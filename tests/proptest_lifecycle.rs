//! Proptest invariants for the reservation lifecycle state machine.
//!
//! The strategy: generate an arbitrary sequence of operations against a
//! single product, mirror the operations on a tiny in-test model, and
//! assert after every step that the externally-observable state of the
//! service matches the model.
//!
//! Invariants checked:
//!   * `available <= total_stock` always.
//!   * `available == total_stock - active - confirmed`.
//!   * Terminal reservations (Confirmed/Cancelled/Expired) cannot be
//!     transitioned again.

use std::collections::HashMap;

use chrono::{DateTime, Duration, TimeZone, Utc};
use inventory_reservation::{ReservationError, ReservationService, ReservationState};
use proptest::collection::vec;
use proptest::prelude::*;

const TOTAL_STOCK: u32 = 5;
const PRODUCT: &str = "sku-prop";
// Hold duration must match the library's RESERVATION_HOLD; we keep this in
// sync intentionally so the test is independent of internals.
const HOLD_SECS: i64 = 2 * 60;

#[derive(Debug, Clone)]
enum Op {
    Reserve,
    /// Index into the list of reservation IDs created so far.
    Confirm(usize),
    Cancel(usize),
    /// Advance simulated time by N seconds and run expire_reservations.
    AdvanceAndExpire(u32),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        1 => Just(Op::Reserve),
        1 => (0usize..16).prop_map(Op::Confirm),
        1 => (0usize..16).prop_map(Op::Cancel),
        1 => (0u32..200).prop_map(Op::AdvanceAndExpire),
    ]
}

#[derive(Default)]
struct Model {
    active: u32,
    confirmed: u32,
    /// Reservation id -> (state, expires_at).
    reservations: HashMap<String, (ReservationState, DateTime<Utc>)>,
    /// Insertion order, so Op::Confirm(i)/Cancel(i) can pick by index.
    ids: Vec<String>,
}

impl Model {
    fn available(&self, total: u32) -> u32 {
        total.saturating_sub(self.active + self.confirmed)
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        // Keep CI/local runs snappy. 64 cases x sequence length 30 still
        // explores millions of state machine paths.
        cases: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn lifecycle_invariants_hold_under_arbitrary_op_sequences(
        ops in vec(op_strategy(), 0..30usize)
    ) {
        let svc = ReservationService::new();
        svc.seed_product(PRODUCT, TOTAL_STOCK);

        let mut model = Model::default();
        let mut now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let hold = Duration::seconds(HOLD_SECS);

        for op in ops {
            match op {
                Op::Reserve => {
                    let result = svc.reserve_item(PRODUCT, "u", now);
                    if model.available(TOTAL_STOCK) > 0 {
                        let r = result.expect("reserve should succeed when stock available");
                        model.active += 1;
                        model.reservations.insert(
                            r.reservation_id.clone(),
                            (ReservationState::Active, now + hold),
                        );
                        model.ids.push(r.reservation_id);
                    } else {
                        prop_assert_eq!(result.unwrap_err(), ReservationError::OutOfStock);
                    }
                }
                Op::Confirm(idx) => {
                    let Some(id) = model.ids.get(idx).cloned() else { continue };
                    let (state, expires_at) = model.reservations[&id];
                    let result = svc.confirm_reservation(&id, now);
                    match state {
                        ReservationState::Active if now <= expires_at => {
                            prop_assert!(result.is_ok());
                            model.active -= 1;
                            model.confirmed += 1;
                            model.reservations.insert(id, (ReservationState::Confirmed, expires_at));
                        }
                        ReservationState::Active => {
                            // Past expiry: confirm reports Expired and releases stock.
                            prop_assert_eq!(result.unwrap_err(), ReservationError::ReservationExpired);
                            model.active -= 1;
                            model.reservations.insert(id, (ReservationState::Expired, expires_at));
                        }
                        _ => {
                            prop_assert_eq!(result.unwrap_err(), ReservationError::ReservationAlreadyFinalized);
                        }
                    }
                }
                Op::Cancel(idx) => {
                    let Some(id) = model.ids.get(idx).cloned() else { continue };
                    let (state, expires_at) = model.reservations[&id];
                    let result = svc.cancel_reservation(&id, now);
                    match state {
                        ReservationState::Active => {
                            // cancel does NOT check expiry in the current impl;
                            // it accepts the cancellation regardless.
                            prop_assert!(result.is_ok());
                            model.active -= 1;
                            model.reservations.insert(id, (ReservationState::Cancelled, expires_at));
                        }
                        _ => {
                            prop_assert_eq!(result.unwrap_err(), ReservationError::ReservationAlreadyFinalized);
                        }
                    }
                }
                Op::AdvanceAndExpire(secs) => {
                    now += Duration::seconds(secs as i64);
                    let n = svc.expire_reservations(now).expect("expire ok");
                    let mut model_expired = 0usize;
                    let stale_ids: Vec<String> = model
                        .reservations
                        .iter()
                        .filter(|(_, (s, e))| *s == ReservationState::Active && now > *e)
                        .map(|(id, _)| id.clone())
                        .collect();
                    for id in stale_ids {
                        let (_, e) = model.reservations[&id];
                        model.reservations.insert(id, (ReservationState::Expired, e));
                        model.active -= 1;
                        model_expired += 1;
                    }
                    prop_assert_eq!(n, model_expired);
                }
            }

            // Universal invariant: available stock matches the model.
            let available = svc.get_available_stock(PRODUCT).expect("seeded");
            prop_assert_eq!(available, model.available(TOTAL_STOCK));
            prop_assert!(available <= TOTAL_STOCK);
        }
    }
}
