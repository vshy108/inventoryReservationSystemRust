use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use parking_lot::Mutex;

use crate::domain::{Reservation, ReservationError, ReservationState};

/// Reservation hold time per spec §2.
const RESERVATION_HOLD: Duration = Duration::minutes(2);

/// All state for a single product, including its own reservations.
///
/// Co-locating reservations with their product means a single per-product
/// `Mutex` is enough to make the read-check-write sequence atomic for both
/// stock counters and reservation lifecycle. No reservation ever needs two
/// locks held at the same time, which removes any deadlock surface.
#[derive(Debug)]
struct ProductState {
    total_stock: u32,
    confirmed_count: u32,
    active_reservation_count: u32,
    reservations: HashMap<String, Reservation>,
}

impl ProductState {
    fn new(total_stock: u32) -> Self {
        Self {
            total_stock,
            confirmed_count: 0,
            active_reservation_count: 0,
            reservations: HashMap::new(),
        }
    }

    fn available(&self) -> u32 {
        // Saturating to avoid underflow if invariants are ever violated;
        // counters should never exceed total_stock by construction.
        self.total_stock
            .saturating_sub(self.confirmed_count)
            .saturating_sub(self.active_reservation_count)
    }
}

/// Per-product locked state. Wrapping the mutex in `Arc` lets callers obtain
/// the lock handle from `DashMap` without holding a shard read guard.
type ProductLock = Arc<Mutex<ProductState>>;

/// In-memory reservation service with per-product locking.
///
/// Concurrency model:
/// - `products` is a `DashMap` of per-product `Mutex<ProductState>`.
///   Different SKUs reserve in parallel.
/// - `reservation_index` is a `DashMap` mapping `reservation_id -> product_id`
///   so confirm/cancel can route to the correct product lock without a
///   global mutex.
/// - Lock discipline: at most one product mutex is held at any instant
///   (the index is read-only by the time the product mutex is taken).
#[derive(Debug)]
pub struct ReservationService {
    products: DashMap<String, ProductLock>,
    reservation_index: DashMap<String, String>,
    next_id: AtomicU64,
}

impl Default for ReservationService {
    fn default() -> Self {
        Self::new()
    }
}

impl ReservationService {
    /// Constructs an empty service with no products registered.
    pub fn new() -> Self {
        Self {
            products: DashMap::new(),
            reservation_index: DashMap::new(),
            next_id: AtomicU64::new(0),
        }
    }

    /// Registers (or re-registers) a product with the given total stock.
    /// Resets counters and clears that product's reservations. On re-seed,
    /// stale entries in `reservation_index` that pointed at this product
    /// are removed so the index does not leak across product generations.
    pub fn seed_product(&self, product_id: &str, total_stock: u32) {
        // Drop any stale index entries pointing at this product BEFORE
        // we install the fresh state, so concurrent readers never see a
        // mix of "fresh product, stale index". `retain` holds only the
        // DashMap shard locks — never the product mutex.
        self.reservation_index
            .retain(|_, pid| pid.as_str() != product_id);

        self.products.insert(
            product_id.to_owned(),
            Arc::new(Mutex::new(ProductState::new(total_stock))),
        );
    }

    /// Reserves one unit of `product_id` for `user_id`, returning a fresh
    /// `Active` [`Reservation`] with `expires_at = now + RESERVATION_HOLD`.
    ///
    /// # Errors
    /// * [`ReservationError::ProductNotFound`] if the product was never seeded.
    /// * [`ReservationError::OutOfStock`] if `available_stock == 0`.
    #[must_use = "the returned Reservation owns the held unit; dropping the id without confirm/cancel will leak stock until it expires"]
    pub fn reserve_item(
        &self,
        product_id: &str,
        user_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Reservation, ReservationError> {
        let lock = self.product_lock(product_id)?;
        let mut product = lock.lock();

        if product.available() == 0 {
            return Err(ReservationError::OutOfStock);
        }

        product.active_reservation_count += 1;

        let reservation = Reservation {
            reservation_id: self.next_reservation_id(),
            product_id: product_id.to_owned(),
            user_id: user_id.to_owned(),
            state: ReservationState::Active,
            created_at: now,
            expires_at: now + RESERVATION_HOLD,
            confirmed_at: None,
            cancelled_at: None,
            expired_at: None,
        };

        product
            .reservations
            .insert(reservation.reservation_id.clone(), reservation.clone());
        // Index insert is safe to do while still holding the product lock:
        // DashMap takes only its own per-shard lock, never the product mutex.
        self.reservation_index
            .insert(reservation.reservation_id.clone(), product_id.to_owned());

        Ok(reservation)
    }

    /// Transitions an `Active` reservation to `Confirmed`.
    ///
    /// # Errors
    /// * [`ReservationError::ReservationNotFound`] if the id is unknown.
    /// * [`ReservationError::ReservationAlreadyFinalized`] if already in a terminal state.
    /// * [`ReservationError::ReservationExpired`] if `now > expires_at`. As a side
    ///   effect, the reservation is transitioned to `Expired` and stock is released,
    ///   so callers cannot retry-and-confirm a stale `Active` reservation.
    #[must_use = "the Result encodes ReservationExpired / AlreadyFinalized / NotFound; ignoring it can hide a billing bug"]
    pub fn confirm_reservation(
        &self,
        reservation_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Reservation, ReservationError> {
        let lock = self.lock_for_reservation(reservation_id)?;
        let mut product = lock.lock();

        // Snapshot just what's needed to decide the transition, so the
        // mutable borrow of `product.reservations` is released before we
        // mutate sibling fields on `product`.
        let (state, expires_at) = {
            let r = product
                .reservations
                .get(reservation_id)
                .ok_or(ReservationError::ReservationNotFound)?;
            (r.state, r.expires_at)
        };

        match state {
            ReservationState::Active => {}
            ReservationState::Confirmed
            | ReservationState::Cancelled
            | ReservationState::Expired => {
                return Err(ReservationError::ReservationAlreadyFinalized);
            }
        }

        if now > expires_at {
            // Past expiry: mark Expired, release stock, and report
            // ReservationExpired so callers cannot ride a stale Active state.
            release_active_to_expired(&mut product, reservation_id, now);
            return Err(ReservationError::ReservationExpired);
        }

        // Active -> Confirmed: a held unit becomes a confirmed sale; the
        // total occupied stock count is unchanged.
        product.active_reservation_count -= 1;
        product.confirmed_count += 1;
        let r = product
            .reservations
            .get_mut(reservation_id)
            .ok_or(ReservationError::ReservationNotFound)?;
        r.state = ReservationState::Confirmed;
        r.confirmed_at = Some(now);
        Ok(r.clone())
    }

    /// Transitions an `Active` reservation to `Cancelled`, releasing the held unit.
    ///
    /// # Errors
    /// * [`ReservationError::ReservationNotFound`] if the id is unknown.
    /// * [`ReservationError::ReservationAlreadyFinalized`] if already in a terminal state.
    #[must_use = "the Result encodes AlreadyFinalized / NotFound; ignoring it can hide a state-machine bug"]
    pub fn cancel_reservation(
        &self,
        reservation_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Reservation, ReservationError> {
        let lock = self.lock_for_reservation(reservation_id)?;
        let mut product = lock.lock();

        let state = product
            .reservations
            .get(reservation_id)
            .ok_or(ReservationError::ReservationNotFound)?
            .state;

        match state {
            ReservationState::Active => {}
            ReservationState::Confirmed
            | ReservationState::Cancelled
            | ReservationState::Expired => {
                return Err(ReservationError::ReservationAlreadyFinalized);
            }
        }

        product.active_reservation_count -= 1;
        let r = product
            .reservations
            .get_mut(reservation_id)
            .ok_or(ReservationError::ReservationNotFound)?;
        r.state = ReservationState::Cancelled;
        r.cancelled_at = Some(now);
        Ok(r.clone())
    }

    /// Sweeps every product, transitioning `Active` reservations whose
    /// `expires_at < now` to `Expired` and releasing their stock. Returns
    /// the count of newly-expired reservations across all products.
    ///
    /// Each product is processed under only its own lock, so SKUs can be
    /// swept without blocking each other.
    pub fn expire_reservations(&self, now: DateTime<Utc>) -> Result<usize, ReservationError> {
        let mut expired = 0usize;
        // Each product is processed under only its own lock, so different
        // SKUs can be swept in parallel by callers if needed.
        for entry in self.products.iter() {
            let mut product = entry.value().lock();
            let candidate_ids: Vec<String> = product
                .reservations
                .iter()
                .filter(|(_, r)| r.state == ReservationState::Active && now > r.expires_at)
                .map(|(id, _)| id.clone())
                .collect();
            for id in candidate_ids {
                release_active_to_expired(&mut product, &id, now);
                expired += 1;
            }
        }
        Ok(expired)
    }

    /// Returns a snapshot of the reservation, or `None` if the id is unknown.
    pub fn get_reservation(&self, reservation_id: &str) -> Option<Reservation> {
        let lock = self.lock_for_reservation(reservation_id).ok()?;
        let product = lock.lock();
        product.reservations.get(reservation_id).cloned()
    }

    /// Computes `total_stock - confirmed_count - active_reservation_count`
    /// for `product_id`. Saturates at zero if invariants are ever violated
    /// (which they should not be, by construction).
    ///
    /// # Errors
    /// * [`ReservationError::ProductNotFound`] if the product was never seeded.
    pub fn get_available_stock(&self, product_id: &str) -> Result<u32, ReservationError> {
        let lock = self.product_lock(product_id)?;
        let product = lock.lock();
        Ok(product.available())
    }

    fn product_lock(&self, product_id: &str) -> Result<ProductLock, ReservationError> {
        self.products
            .get(product_id)
            .map(|r| Arc::clone(r.value()))
            .ok_or(ReservationError::ProductNotFound)
    }

    fn lock_for_reservation(&self, reservation_id: &str) -> Result<ProductLock, ReservationError> {
        let product_id = self
            .reservation_index
            .get(reservation_id)
            .ok_or(ReservationError::ReservationNotFound)?
            .value()
            .clone();
        self.product_lock(&product_id)
    }

    fn next_reservation_id(&self) -> String {
        let n = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("rsv-{n}")
    }
}

/// Transition an Active reservation into Expired and release its hold on stock.
/// Caller must hold the product's mutex and have validated `Active` state.
fn release_active_to_expired(product: &mut ProductState, reservation_id: &str, now: DateTime<Utc>) {
    product.active_reservation_count -= 1;
    if let Some(r) = product.reservations.get_mut(reservation_id) {
        r.state = ReservationState::Expired;
        r.expired_at = Some(now);
    }
}
