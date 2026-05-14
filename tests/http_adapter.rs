//! Tests for S2 — HTTP Adapter.
//!
//! Covers all endpoints defined in `specs/http-adapter.md`: success paths,
//! error mapping from `ReservationError` to HTTP status codes, and input
//! validation at the HTTP edge.

use std::sync::Arc;

use chrono::{Duration, Utc};
use inventory_reservation::{
    ReservationService, ReservationState,
    interface::http::{HttpAdapter, ReserveRequest},
};

fn adapter() -> HttpAdapter {
    HttpAdapter::new(Arc::new(ReservationService::new()))
}

fn seeded_adapter(stock: u32) -> (HttpAdapter, String) {
    let svc = Arc::new(ReservationService::new());
    svc.seed_product("sku-1", stock);
    (HttpAdapter::new(svc), "sku-1".into())
}

// ---------------------------------------------------------------------------
// Reserve
// ---------------------------------------------------------------------------

#[test]
fn reserve_success_returns_201() {
    let (adapter, product_id) = seeded_adapter(1);
    let now = Utc::now();

    let result = adapter.reserve(&product_id, ReserveRequest { user_id: "alice".into() }, now);

    let resp = result.expect("expected Ok");
    assert_eq!(resp.status, 201);
    assert_eq!(resp.body.product_id, "sku-1");
    assert_eq!(resp.body.user_id, "alice");
    assert_eq!(resp.body.state, ReservationState::Active);
}

#[test]
fn reserve_product_not_found_returns_404() {
    let adapter = adapter();
    let now = Utc::now();

    let err = adapter
        .reserve("no-sku", ReserveRequest { user_id: "alice".into() }, now)
        .unwrap_err();

    assert_eq!(err.status, 404);
}

#[test]
fn reserve_out_of_stock_returns_409() {
    let (adapter, product_id) = seeded_adapter(0);
    let now = Utc::now();

    let err = adapter
        .reserve(&product_id, ReserveRequest { user_id: "alice".into() }, now)
        .unwrap_err();

    assert_eq!(err.status, 409);
}

#[test]
fn reserve_empty_user_id_returns_400() {
    let (adapter, product_id) = seeded_adapter(1);
    let now = Utc::now();

    let err = adapter
        .reserve(&product_id, ReserveRequest { user_id: "".into() }, now)
        .unwrap_err();

    assert_eq!(err.status, 400);
}

#[test]
fn reserve_empty_product_id_returns_400() {
    let adapter = adapter();
    let now = Utc::now();

    let err = adapter
        .reserve("", ReserveRequest { user_id: "alice".into() }, now)
        .unwrap_err();

    assert_eq!(err.status, 400);
}

// ---------------------------------------------------------------------------
// Confirm
// ---------------------------------------------------------------------------

#[test]
fn confirm_success_returns_200() {
    let (adapter, product_id) = seeded_adapter(1);
    let now = Utc::now();
    let reserve_resp = adapter
        .reserve(&product_id, ReserveRequest { user_id: "alice".into() }, now)
        .unwrap();

    let result = adapter.confirm(&reserve_resp.body.reservation_id, now);

    let resp = result.expect("expected Ok");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body.state, ReservationState::Confirmed);
}

#[test]
fn confirm_not_found_returns_404() {
    let adapter = adapter();
    let err = adapter.confirm("no-such-id", Utc::now()).unwrap_err();
    assert_eq!(err.status, 404);
}

#[test]
fn confirm_expired_returns_409() {
    let (adapter, product_id) = seeded_adapter(1);
    let past = Utc::now() - Duration::minutes(10);
    let reserve_resp = adapter
        .reserve(&product_id, ReserveRequest { user_id: "alice".into() }, past)
        .unwrap();

    let err = adapter
        .confirm(&reserve_resp.body.reservation_id, Utc::now())
        .unwrap_err();

    assert_eq!(err.status, 409);
}

#[test]
fn confirm_already_finalized_returns_409() {
    let (adapter, product_id) = seeded_adapter(1);
    let now = Utc::now();
    let r = adapter
        .reserve(&product_id, ReserveRequest { user_id: "alice".into() }, now)
        .unwrap();
    adapter.confirm(&r.body.reservation_id, now).unwrap();

    let err = adapter.confirm(&r.body.reservation_id, now).unwrap_err();

    assert_eq!(err.status, 409);
}

#[test]
fn confirm_empty_reservation_id_returns_400() {
    let adapter = adapter();
    let err = adapter.confirm("", Utc::now()).unwrap_err();
    assert_eq!(err.status, 400);
}

// ---------------------------------------------------------------------------
// Cancel
// ---------------------------------------------------------------------------

#[test]
fn cancel_success_returns_200() {
    let (adapter, product_id) = seeded_adapter(1);
    let now = Utc::now();
    let r = adapter
        .reserve(&product_id, ReserveRequest { user_id: "alice".into() }, now)
        .unwrap();

    let resp = adapter.cancel(&r.body.reservation_id, now).unwrap();

    assert_eq!(resp.status, 200);
    assert_eq!(resp.body.state, ReservationState::Cancelled);
}

#[test]
fn cancel_already_finalized_returns_409() {
    let (adapter, product_id) = seeded_adapter(1);
    let now = Utc::now();
    let r = adapter
        .reserve(&product_id, ReserveRequest { user_id: "alice".into() }, now)
        .unwrap();
    adapter.cancel(&r.body.reservation_id, now).unwrap();

    let err = adapter.cancel(&r.body.reservation_id, now).unwrap_err();

    assert_eq!(err.status, 409);
}

// ---------------------------------------------------------------------------
// Get stock
// ---------------------------------------------------------------------------

#[test]
fn get_stock_success_returns_available() {
    let (adapter, product_id) = seeded_adapter(5);

    let resp = adapter.get_stock(&product_id).unwrap();

    assert_eq!(resp.status, 200);
    assert_eq!(resp.body.available, 5);
}

#[test]
fn get_stock_product_not_found_returns_404() {
    let adapter = adapter();
    let err = adapter.get_stock("no-sku").unwrap_err();
    assert_eq!(err.status, 404);
}

// ---------------------------------------------------------------------------
// Get reservation
// ---------------------------------------------------------------------------

#[test]
fn get_reservation_success_returns_200() {
    let (adapter, product_id) = seeded_adapter(1);
    let now = Utc::now();
    let r = adapter
        .reserve(&product_id, ReserveRequest { user_id: "alice".into() }, now)
        .unwrap();

    let resp = adapter.get_reservation(&r.body.reservation_id).unwrap();

    assert_eq!(resp.status, 200);
    assert_eq!(resp.body.reservation_id, r.body.reservation_id);
}

#[test]
fn get_reservation_not_found_returns_404() {
    let adapter = adapter();
    let err = adapter.get_reservation("no-such").unwrap_err();
    assert_eq!(err.status, 404);
}

// ---------------------------------------------------------------------------
// Expire sweep
// ---------------------------------------------------------------------------

#[test]
fn expire_returns_count_of_swept_reservations() {
    let (adapter, product_id) = seeded_adapter(2);
    let past = Utc::now() - Duration::minutes(10);
    adapter
        .reserve(&product_id, ReserveRequest { user_id: "alice".into() }, past)
        .unwrap();
    adapter
        .reserve(&product_id, ReserveRequest { user_id: "bob".into() }, past)
        .unwrap();

    let resp = adapter.expire(Utc::now()).unwrap();

    assert_eq!(resp.status, 200);
    assert_eq!(resp.body.expired_count, 2);
}
