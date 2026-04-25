use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;

use crate::domain::{Reservation, ReservationError, ReservationState};

/// Reservation hold time per spec §2.
const RESERVATION_HOLD: Duration = Duration::minutes(2);

#[derive(Debug)]
struct ProductState {
    total_stock: u32,
    confirmed_count: u32,
    active_reservation_count: u32,
}

impl ProductState {
    fn available(&self) -> u32 {
        // Saturating to avoid underflow if invariants are ever violated;
        // counters should never exceed total_stock by construction.
        self.total_stock
            .saturating_sub(self.confirmed_count)
            .saturating_sub(self.active_reservation_count)
    }
}

#[derive(Debug)]
struct State {
    products: HashMap<String, ProductState>,
    reservations: HashMap<String, Reservation>,
}

impl State {
    fn new() -> Self {
        Self {
            products: HashMap::new(),
            reservations: HashMap::new(),
        }
    }
}

/// In-memory reservation service.
///
/// L1/L2: a single coarse `Mutex` guards all state. L3 will refine to
/// per-product locking for parallel different-SKU throughput.
pub struct ReservationService {
    state: Mutex<State>,
    next_id: AtomicU64,
}

impl Default for ReservationService {
    fn default() -> Self {
        Self::new()
    }
}

impl ReservationService {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::new()),
            next_id: AtomicU64::new(0),
        }
    }

    /// Registers (or re-registers) a product with the given total stock.
    /// Resets counters for that product. Existing reservations for the
    /// product are not removed (L1/L2 only call this in fresh services).
    pub fn seed_product(&self, product_id: &str, total_stock: u32) {
        let mut state = self.state.lock();
        state.products.insert(
            product_id.to_owned(),
            ProductState {
                total_stock,
                confirmed_count: 0,
                active_reservation_count: 0,
            },
        );
    }

    pub fn reserve_item(
        &self,
        product_id: &str,
        user_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Reservation, ReservationError> {
        let mut state = self.state.lock();

        let product = state
            .products
            .get_mut(product_id)
            .ok_or(ReservationError::ProductNotFound)?;

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

        state
            .reservations
            .insert(reservation.reservation_id.clone(), reservation.clone());
        Ok(reservation)
    }

    pub fn confirm_reservation(
        &self,
        reservation_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Reservation, ReservationError> {
        let mut guard = self.state.lock();
        let State {
            products,
            reservations,
        } = &mut *guard;

        let reservation = reservations
            .get_mut(reservation_id)
            .ok_or(ReservationError::ReservationNotFound)?;

        match reservation.state {
            ReservationState::Active => {}
            ReservationState::Confirmed
            | ReservationState::Cancelled
            | ReservationState::Expired => {
                return Err(ReservationError::ReservationAlreadyFinalized);
            }
        }

        let product = products
            .get_mut(&reservation.product_id)
            .ok_or(ReservationError::ProductNotFound)?;

        if now > reservation.expires_at {
            // Past expiry: mark Expired, release stock, and report
            // ReservationExpired so callers cannot ride a stale Active state.
            transition_to_expired(reservation, product, now);
            return Err(ReservationError::ReservationExpired);
        }

        // Active -> Confirmed: a held unit becomes a confirmed sale; the
        // total occupied stock count is unchanged.
        product.active_reservation_count -= 1;
        product.confirmed_count += 1;
        reservation.state = ReservationState::Confirmed;
        reservation.confirmed_at = Some(now);

        Ok(reservation.clone())
    }

    pub fn cancel_reservation(
        &self,
        reservation_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Reservation, ReservationError> {
        let mut guard = self.state.lock();
        let State {
            products,
            reservations,
        } = &mut *guard;

        let reservation = reservations
            .get_mut(reservation_id)
            .ok_or(ReservationError::ReservationNotFound)?;

        match reservation.state {
            ReservationState::Active => {}
            ReservationState::Confirmed
            | ReservationState::Cancelled
            | ReservationState::Expired => {
                return Err(ReservationError::ReservationAlreadyFinalized);
            }
        }

        let product = products
            .get_mut(&reservation.product_id)
            .ok_or(ReservationError::ProductNotFound)?;

        product.active_reservation_count -= 1;
        reservation.state = ReservationState::Cancelled;
        reservation.cancelled_at = Some(now);

        Ok(reservation.clone())
    }

    pub fn expire_reservations(&self, now: DateTime<Utc>) -> Result<usize, ReservationError> {
        let mut guard = self.state.lock();
        let State {
            products,
            reservations,
        } = &mut *guard;

        let mut expired = 0usize;
        for reservation in reservations.values_mut() {
            if reservation.state != ReservationState::Active {
                continue;
            }
            if now <= reservation.expires_at {
                continue;
            }
            // An Active reservation always has its product registered.
            if let Some(product) = products.get_mut(&reservation.product_id) {
                transition_to_expired(reservation, product, now);
                expired += 1;
            }
        }
        Ok(expired)
    }

    pub fn get_reservation(&self, reservation_id: &str) -> Option<Reservation> {
        self.state.lock().reservations.get(reservation_id).cloned()
    }

    pub fn get_available_stock(&self, product_id: &str) -> Result<u32, ReservationError> {
        let guard = self.state.lock();
        guard
            .products
            .get(product_id)
            .map(ProductState::available)
            .ok_or(ReservationError::ProductNotFound)
    }

    fn next_reservation_id(&self) -> String {
        let n = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("rsv-{n}")
    }
}

/// Transition an Active reservation into Expired and release its hold on stock.
/// Caller must have already validated that `reservation.state == Active`.
fn transition_to_expired(
    reservation: &mut Reservation,
    product: &mut ProductState,
    now: DateTime<Utc>,
) {
    product.active_reservation_count -= 1;
    reservation.state = ReservationState::Expired;
    reservation.expired_at = Some(now);
}
