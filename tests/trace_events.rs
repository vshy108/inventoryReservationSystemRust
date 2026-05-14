//! Tests for S3 — Reservation Trace Events.
//!
//! Verifies that [`ReservationService`] emits the correct [`TraceEvent`] variants
//! on every success and rejection path, in the correct order, with the correct payloads.
//! Events are collected via a thread-safe [`Recorder`] injected through the
//! [`TraceSubscriber`] port.

use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use inventory_reservation::{
    ReservationError, ReservationService, TraceEvent, TraceSubscriber,
};

// ---------------------------------------------------------------------------
// Test recorder
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Recorder(Mutex<Vec<TraceEvent>>);

impl Recorder {
    fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Vec::new())))
    }

    fn events(&self) -> Vec<TraceEvent> {
        self.0.lock().unwrap().clone()
    }
}

impl TraceSubscriber for Recorder {
    fn record(&self, event: TraceEvent) {
        self.0.lock().unwrap().push(event);
    }
}

fn svc_with_recorder() -> (ReservationService, Arc<Recorder>) {
    let rec = Recorder::new();
    let svc = ReservationService::new()
        .with_subscriber(Arc::clone(&rec) as Arc<dyn TraceSubscriber>);
    (svc, rec)
}

// ---------------------------------------------------------------------------
// Reserve paths
// ---------------------------------------------------------------------------

#[test]
fn reserved_event_on_success() {
    let (svc, rec) = svc_with_recorder();
    svc.seed_product("sku-1", 1);

    let r = svc.reserve_item("sku-1", "alice", Utc::now()).unwrap();

    let events = rec.events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        TraceEvent::Reserved {
            reservation_id: r.reservation_id.clone(),
            product_id: "sku-1".into(),
            user_id: "alice".into(),
        }
    );
}

#[test]
fn reserve_failed_event_on_out_of_stock() {
    let (svc, rec) = svc_with_recorder();
    svc.seed_product("sku-1", 0);

    svc.reserve_item("sku-1", "alice", Utc::now()).unwrap_err();

    let events = rec.events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        TraceEvent::ReserveFailed {
            product_id: "sku-1".into(),
            user_id: "alice".into(),
            error: ReservationError::OutOfStock,
        }
    );
}

#[test]
fn reserve_failed_event_on_product_not_found() {
    let (svc, rec) = svc_with_recorder();

    svc.reserve_item("no-such-sku", "alice", Utc::now())
        .unwrap_err();

    let events = rec.events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        TraceEvent::ReserveFailed {
            product_id: "no-such-sku".into(),
            user_id: "alice".into(),
            error: ReservationError::ProductNotFound,
        }
    );
}

// ---------------------------------------------------------------------------
// Confirm paths
// ---------------------------------------------------------------------------

#[test]
fn confirmed_event_on_success() {
    let (svc, rec) = svc_with_recorder();
    svc.seed_product("sku-1", 1);
    let r = svc.reserve_item("sku-1", "alice", Utc::now()).unwrap();
    // clear reserve event
    rec.0.lock().unwrap().clear();

    svc.confirm_reservation(&r.reservation_id, Utc::now())
        .unwrap();

    let events = rec.events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        TraceEvent::Confirmed {
            reservation_id: r.reservation_id.clone(),
        }
    );
}

#[test]
fn confirm_failed_event_on_expired() {
    let (svc, rec) = svc_with_recorder();
    svc.seed_product("sku-1", 1);
    let past = Utc::now() - Duration::minutes(10);
    let r = svc.reserve_item("sku-1", "alice", past).unwrap();
    rec.0.lock().unwrap().clear();

    svc.confirm_reservation(&r.reservation_id, Utc::now())
        .unwrap_err();

    let events = rec.events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        TraceEvent::ConfirmFailed {
            reservation_id: r.reservation_id.clone(),
            error: ReservationError::ReservationExpired,
        }
    );
}

#[test]
fn confirm_failed_event_on_already_finalized() {
    let (svc, rec) = svc_with_recorder();
    svc.seed_product("sku-1", 1);
    let r = svc.reserve_item("sku-1", "alice", Utc::now()).unwrap();
    svc.confirm_reservation(&r.reservation_id, Utc::now())
        .unwrap();
    rec.0.lock().unwrap().clear();

    svc.confirm_reservation(&r.reservation_id, Utc::now())
        .unwrap_err();

    let events = rec.events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        TraceEvent::ConfirmFailed {
            reservation_id: r.reservation_id.clone(),
            error: ReservationError::ReservationAlreadyFinalized,
        }
    );
}

// ---------------------------------------------------------------------------
// Cancel paths
// ---------------------------------------------------------------------------

#[test]
fn cancelled_event_on_success() {
    let (svc, rec) = svc_with_recorder();
    svc.seed_product("sku-1", 1);
    let r = svc.reserve_item("sku-1", "alice", Utc::now()).unwrap();
    rec.0.lock().unwrap().clear();

    svc.cancel_reservation(&r.reservation_id, Utc::now())
        .unwrap();

    let events = rec.events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        TraceEvent::Cancelled {
            reservation_id: r.reservation_id.clone(),
        }
    );
}

#[test]
fn cancel_failed_event_on_already_finalized() {
    let (svc, rec) = svc_with_recorder();
    svc.seed_product("sku-1", 1);
    let r = svc.reserve_item("sku-1", "alice", Utc::now()).unwrap();
    svc.cancel_reservation(&r.reservation_id, Utc::now())
        .unwrap();
    rec.0.lock().unwrap().clear();

    svc.cancel_reservation(&r.reservation_id, Utc::now())
        .unwrap_err();

    let events = rec.events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0],
        TraceEvent::CancelFailed {
            reservation_id: r.reservation_id.clone(),
            error: ReservationError::ReservationAlreadyFinalized,
        }
    );
}

// ---------------------------------------------------------------------------
// Expire sweep
// ---------------------------------------------------------------------------

#[test]
fn expired_events_emitted_by_expire_sweep() {
    let (svc, rec) = svc_with_recorder();
    svc.seed_product("sku-1", 3);

    let past = Utc::now() - Duration::minutes(10);
    let r1 = svc.reserve_item("sku-1", "alice", past).unwrap();
    let r2 = svc.reserve_item("sku-1", "bob", past).unwrap();
    // r3 reserved now — should NOT expire yet
    let _r3 = svc.reserve_item("sku-1", "carol", Utc::now()).unwrap();
    rec.0.lock().unwrap().clear();

    let count = svc.expire_reservations(Utc::now()).unwrap();

    assert_eq!(count, 2);
    let events = rec.events();
    assert_eq!(events.len(), 2);

    let expired_ids: Vec<&str> = events
        .iter()
        .map(|e| match e {
            TraceEvent::Expired { reservation_id } => reservation_id.as_str(),
            other => panic!("unexpected event: {other:?}"),
        })
        .collect();

    assert!(expired_ids.contains(&r1.reservation_id.as_str()));
    assert!(expired_ids.contains(&r2.reservation_id.as_str()));
}

// ---------------------------------------------------------------------------
// Event ordering on a full happy-path lifecycle
// ---------------------------------------------------------------------------

#[test]
fn full_lifecycle_event_order() {
    let (svc, rec) = svc_with_recorder();
    svc.seed_product("sku-1", 1);

    let now = Utc::now();
    let r = svc.reserve_item("sku-1", "alice", now).unwrap();
    svc.confirm_reservation(&r.reservation_id, now).unwrap();

    let events = rec.events();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[0], TraceEvent::Reserved { .. }));
    assert!(matches!(&events[1], TraceEvent::Confirmed { .. }));
}
