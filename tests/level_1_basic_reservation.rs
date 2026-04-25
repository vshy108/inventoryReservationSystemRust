//! Level 1 — Basic Inventory Reservation
//!
//! Spec: ../specs/level-1-basic-reservation.md

use chrono::Utc;
use inventory_reservation::{ReservationError, ReservationService, ReservationState};

fn service_with_stock(product_id: &str, total_stock: u32) -> ReservationService {
    let svc = ReservationService::new();
    svc.seed_product(product_id, total_stock);
    svc
}

#[test]
fn l1_a_reserves_when_stock_is_available() {
    let svc = service_with_stock("sku-1", 1);

    let reservation = svc
        .reserve_item("sku-1", "user-a", Utc::now())
        .expect("reserve should succeed when stock is available");

    assert_eq!(reservation.product_id, "sku-1");
    assert_eq!(reservation.user_id, "user-a");
    assert_eq!(reservation.state, ReservationState::Active);
}

#[test]
fn l1_b_rejects_when_no_stock() {
    let svc = service_with_stock("sku-1", 0);

    let err = svc
        .reserve_item("sku-1", "user-a", Utc::now())
        .expect_err("reserve should fail when stock is zero");

    assert_eq!(err, ReservationError::OutOfStock);
}

#[test]
fn l1_c_one_winner_for_the_last_item() {
    let svc = service_with_stock("sku-1", 1);
    let now = Utc::now();

    svc.reserve_item("sku-1", "user-a", now)
        .expect("user A should reserve the last unit");
    let b = svc
        .reserve_item("sku-1", "user-b", now)
        .expect_err("user B should be rejected");

    assert_eq!(b, ReservationError::OutOfStock);
}

#[test]
fn l1_d_unknown_product_returns_product_not_found() {
    let svc = ReservationService::new();

    let err = svc
        .reserve_item("missing-sku", "user-a", Utc::now())
        .expect_err("reserve should fail for unknown product");

    assert_eq!(err, ReservationError::ProductNotFound);
}

#[test]
fn default_constructor_matches_new() {
    // `Default` is provided so callers that take `T: Default` (e.g. test
    // harnesses, builders) can construct a service without naming `new`.
    let svc = ReservationService::default();
    svc.seed_product("sku-default", 1);
    assert_eq!(svc.get_available_stock("sku-default").unwrap(), 1);
}
