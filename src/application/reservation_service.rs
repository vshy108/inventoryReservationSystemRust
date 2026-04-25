use chrono::{DateTime, Utc};

use crate::domain::{Reservation, ReservationError};

/// In-memory reservation service.
///
/// L1 only: tracks total stock and active reservation count per product
/// and serves single-unit reservations. L2 (lifecycle) and L3 (concurrency)
/// extend this type.
#[derive(Default)]
pub struct ReservationService {
    // Internals deliberately omitted in the L1 RED phase.
    // Filled in the GREEN phase.
}

impl ReservationService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Test/admin seeding helper: registers a product with the given total stock.
    /// Idempotent semantics will be defined when the GREEN impl lands.
    pub fn seed_product(&self, _product_id: &str, _total_stock: u32) {
        todo!("L1 GREEN: implement product seeding")
    }

    pub fn reserve_item(
        &self,
        _product_id: &str,
        _user_id: &str,
        _now: DateTime<Utc>,
    ) -> Result<Reservation, ReservationError> {
        todo!("L1 GREEN: implement reserve_item")
    }
}
