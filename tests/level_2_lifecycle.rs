//! Level 2 — Reservation Lifecycle & Expiry
//!
//! Spec: ../specs/level-2-lifecycle.md

use chrono::{DateTime, Duration, TimeZone, Utc};
use inventory_reservation::{ReservationError, ReservationService, ReservationState};

fn ts(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).single().expect("valid ts")
}

fn service_with_stock(product_id: &str, total_stock: u32) -> ReservationService {
    let svc = ReservationService::new();
    svc.seed_product(product_id, total_stock);
    svc
}

// --- L2-A: confirm before expiry -------------------------------------------------
#[test]
fn l2_a_confirm_active_within_hold_window() {
    let svc = service_with_stock("sku-1", 1);
    let t0 = ts(1_000);
    let r = svc.reserve_item("sku-1", "user-a", t0).unwrap();

    let t1 = t0 + Duration::seconds(30);
    let confirmed = svc
        .confirm_reservation(&r.reservation_id, t1)
        .expect("confirm should succeed within hold window");

    assert_eq!(confirmed.state, ReservationState::Confirmed);
    assert_eq!(confirmed.confirmed_at, Some(t1));
    // Confirmed sale still consumes one unit.
    assert_eq!(svc.get_available_stock("sku-1").unwrap(), 0);
}

// --- L2-B: confirm after expiry --------------------------------------------------
#[test]
fn l2_b_confirm_after_expiry_fails_and_releases_stock() {
    let svc = service_with_stock("sku-1", 1);
    let t0 = ts(1_000);
    let r = svc.reserve_item("sku-1", "user-a", t0).unwrap();

    // Hold is 2 minutes (per spec §2). Pass 5 minutes.
    let t_after = t0 + Duration::minutes(5);
    let err = svc
        .confirm_reservation(&r.reservation_id, t_after)
        .expect_err("confirm past expires_at must fail");

    assert_eq!(err, ReservationError::ReservationExpired);
    let stored = svc.get_reservation(&r.reservation_id).expect("present");
    assert_eq!(stored.state, ReservationState::Expired);
    // Stock is released so a fresh user can reserve.
    assert_eq!(svc.get_available_stock("sku-1").unwrap(), 1);
}

// --- L2-C: confirm finalized -----------------------------------------------------
#[test]
fn l2_c_confirm_already_finalized_fails() {
    let svc = service_with_stock("sku-1", 1);
    let t0 = ts(1_000);
    let r = svc.reserve_item("sku-1", "user-a", t0).unwrap();
    svc.confirm_reservation(&r.reservation_id, t0).unwrap();

    let err = svc
        .confirm_reservation(&r.reservation_id, t0)
        .expect_err("double-confirm must fail");
    assert_eq!(err, ReservationError::ReservationAlreadyFinalized);
}

// --- L2-D: cancel active --------------------------------------------------------
#[test]
fn l2_d_cancel_active_releases_stock() {
    let svc = service_with_stock("sku-1", 1);
    let t0 = ts(1_000);
    let r = svc.reserve_item("sku-1", "user-a", t0).unwrap();

    let cancelled = svc.cancel_reservation(&r.reservation_id, t0).unwrap();
    assert_eq!(cancelled.state, ReservationState::Cancelled);
    assert_eq!(cancelled.cancelled_at, Some(t0));
    assert_eq!(svc.get_available_stock("sku-1").unwrap(), 1);
}

// --- L2-E: cancel finalized -----------------------------------------------------
#[test]
fn l2_e_cancel_after_confirm_fails() {
    let svc = service_with_stock("sku-1", 1);
    let t0 = ts(1_000);
    let r = svc.reserve_item("sku-1", "user-a", t0).unwrap();
    svc.confirm_reservation(&r.reservation_id, t0).unwrap();

    let err = svc
        .cancel_reservation(&r.reservation_id, t0)
        .expect_err("cancel after confirm must fail");
    assert_eq!(err, ReservationError::ReservationAlreadyFinalized);
}

// --- L2-F: unknown reservation --------------------------------------------------
#[test]
fn l2_f_unknown_reservation_id() {
    let svc = service_with_stock("sku-1", 1);
    let t0 = ts(1_000);

    let err = svc.confirm_reservation("rsv-missing", t0).unwrap_err();
    assert_eq!(err, ReservationError::ReservationNotFound);
    let err = svc.cancel_reservation("rsv-missing", t0).unwrap_err();
    assert_eq!(err, ReservationError::ReservationNotFound);
}

// --- L2-G: expire_reservations sweep --------------------------------------------
#[test]
fn l2_g_expire_reservations_sweeps_only_past_due() {
    let svc = service_with_stock("sku-1", 5);
    let t0 = ts(1_000);

    let old_a = svc.reserve_item("sku-1", "user-a", t0).unwrap();
    let old_b = svc.reserve_item("sku-1", "user-b", t0).unwrap();
    let fresh_t = t0 + Duration::minutes(4);
    let fresh = svc.reserve_item("sku-1", "user-c", fresh_t).unwrap();

    // Sweep at t0+5min: old_a, old_b expire; fresh stays Active (its expires_at = fresh_t + 2m = t0+6m).
    let sweep_at = t0 + Duration::minutes(5);
    let n = svc.expire_reservations(sweep_at).unwrap();

    assert_eq!(n, 2);
    assert_eq!(
        svc.get_reservation(&old_a.reservation_id).unwrap().state,
        ReservationState::Expired
    );
    assert_eq!(
        svc.get_reservation(&old_b.reservation_id).unwrap().state,
        ReservationState::Expired
    );
    assert_eq!(
        svc.get_reservation(&fresh.reservation_id).unwrap().state,
        ReservationState::Active
    );
    // 5 total - 0 confirmed - 1 active = 4 available.
    assert_eq!(svc.get_available_stock("sku-1").unwrap(), 4);

    // Re-running sweep is idempotent (already-expired aren't re-counted).
    assert_eq!(svc.expire_reservations(sweep_at).unwrap(), 0);
}

// --- L2-H: available stock formula ----------------------------------------------
#[test]
fn l2_h_available_stock_subtracts_confirmed_and_active() {
    let svc = service_with_stock("sku-1", 3);
    let t0 = ts(1_000);

    let r1 = svc.reserve_item("sku-1", "user-a", t0).unwrap();
    let _r2 = svc.reserve_item("sku-1", "user-b", t0).unwrap();
    svc.confirm_reservation(&r1.reservation_id, t0).unwrap();

    // 3 total - 1 confirmed - 1 active = 1 available.
    assert_eq!(svc.get_available_stock("sku-1").unwrap(), 1);
}

#[test]
fn l2_h2_get_available_stock_unknown_product() {
    let svc = ReservationService::new();
    assert_eq!(
        svc.get_available_stock("nope").unwrap_err(),
        ReservationError::ProductNotFound
    );
}
