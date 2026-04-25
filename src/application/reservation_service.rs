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
    active_reservation_count: u32,
}

/// In-memory reservation service.
///
/// L1: tracks total stock and active reservation count per product and
/// serves single-unit reservations. A single coarse `Mutex` guards all
/// state — sufficient for L1/L2 correctness; L3 will refine to per-product
/// locking for parallel different-SKU throughput.
#[derive(Default)]
pub struct ReservationService {
    products: Mutex<HashMap<String, ProductState>>,
    next_id: AtomicU64,
}

impl ReservationService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers (or re-registers) a product with the given total stock.
    /// Resets active reservation count for that product.
    pub fn seed_product(&self, product_id: &str, total_stock: u32) {
        // parking_lot::Mutex::lock cannot fail (no poisoning), so no expect()
        // is needed in library code.
        let mut products = self.products.lock();
        products.insert(
            product_id.to_owned(),
            ProductState {
                total_stock,
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
        let mut products = self.products.lock();

        let product = products
            .get_mut(product_id)
            .ok_or(ReservationError::ProductNotFound)?;

        // available_stock = total_stock - active_reservation_count
        // (confirmed_count is introduced in L2).
        if product.active_reservation_count >= product.total_stock {
            return Err(ReservationError::OutOfStock);
        }

        product.active_reservation_count += 1;

        let reservation_id = self.next_reservation_id();
        Ok(Reservation {
            reservation_id,
            product_id: product_id.to_owned(),
            user_id: user_id.to_owned(),
            state: ReservationState::Active,
            created_at: now,
            expires_at: now + RESERVATION_HOLD,
            confirmed_at: None,
            cancelled_at: None,
            expired_at: None,
        })
    }

    pub fn confirm_reservation(
        &self,
        _reservation_id: &str,
        _now: DateTime<Utc>,
    ) -> Result<Reservation, ReservationError> {
        todo!("L2 GREEN: implement confirm_reservation")
    }

    pub fn cancel_reservation(
        &self,
        _reservation_id: &str,
        _now: DateTime<Utc>,
    ) -> Result<Reservation, ReservationError> {
        todo!("L2 GREEN: implement cancel_reservation")
    }

    pub fn expire_reservations(&self, _now: DateTime<Utc>) -> Result<usize, ReservationError> {
        todo!("L2 GREEN: implement expire_reservations")
    }

    pub fn get_reservation(&self, _reservation_id: &str) -> Option<Reservation> {
        todo!("L2 GREEN: implement get_reservation")
    }

    pub fn get_available_stock(&self, _product_id: &str) -> Result<u32, ReservationError> {
        todo!("L2 GREEN: implement get_available_stock")
    }

    fn next_reservation_id(&self) -> String {
        let n = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("rsv-{n}")
    }
}
