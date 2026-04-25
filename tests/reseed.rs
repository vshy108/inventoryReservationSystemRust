//! Re-seed semantics: re-seeding a product MUST clear stale entries in
//! `reservation_index`, so reservation IDs from the previous generation
//! cannot be looked up after the product has been re-seeded.

use chrono::{TimeZone, Utc};
use inventory_reservation::{ReservationError, ReservationService};

#[test]
fn reseed_clears_orphan_reservation_index_entries() {
    let svc = ReservationService::new();
    let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

    svc.seed_product("sku-1", 5);
    let reservation = svc.reserve_item("sku-1", "alice", now).expect("reserve");
    let stale_id = reservation.reservation_id.clone();

    // Re-seed the same product. The previous reservation belongs to a
    // generation that no longer exists.
    svc.seed_product("sku-1", 5);

    // get_reservation should not surface a stale ID.
    assert!(svc.get_reservation(&stale_id).is_none());

    // confirm/cancel should report ReservationNotFound (not ProductNotFound,
    // not "already finalized" — the reservation simply does not exist).
    let confirm_err = svc.confirm_reservation(&stale_id, now).unwrap_err();
    assert_eq!(confirm_err, ReservationError::ReservationNotFound);

    let cancel_err = svc.cancel_reservation(&stale_id, now).unwrap_err();
    assert_eq!(cancel_err, ReservationError::ReservationNotFound);
}
