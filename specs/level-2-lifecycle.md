# Spec — Level 2: Reservation Lifecycle & Expiry

Drives: [tests/level_2_lifecycle.rs](../tests/level_2_lifecycle.rs)

## Goal
Add explicit lifecycle transitions on top of L1: confirm, cancel, and time-based expiry. Make the available-stock formula honour confirmed sales and active reservations.

## Public API additions

```rust
ReservationService::confirm_reservation(
    &self, reservation_id: &str, now: DateTime<Utc>,
) -> Result<Reservation, ReservationError>;

ReservationService::cancel_reservation(
    &self, reservation_id: &str, now: DateTime<Utc>,
) -> Result<Reservation, ReservationError>;

ReservationService::expire_reservations(
    &self, now: DateTime<Utc>,
) -> Result<usize, ReservationError>;

ReservationService::get_reservation(
    &self, reservation_id: &str,
) -> Option<Reservation>;

ReservationService::get_available_stock(
    &self, product_id: &str,
) -> Result<u32, ReservationError>;
```

## Behaviour

| ID    | Given                                        | When                                       | Then                                                                         |
|-------|----------------------------------------------|--------------------------------------------|------------------------------------------------------------------------------|
| L2-A  | active reservation, `now <= expires_at`      | `confirm_reservation`                       | state -> Confirmed; `confirmed_at == now`; available_stock unchanged for that unit |
| L2-B  | active reservation, `now > expires_at`       | `confirm_reservation`                       | `Err(ReservationExpired)`; reservation is **also** marked Expired and stock released |
| L2-C  | already-finalized (Confirmed/Cancelled/Expired) reservation | `confirm_reservation`            | `Err(ReservationAlreadyFinalized)`                                           |
| L2-D  | active reservation                           | `cancel_reservation`                        | state -> Cancelled; active count decreases; available_stock increases by 1   |
| L2-E  | finalized reservation                         | `cancel_reservation`                        | `Err(ReservationAlreadyFinalized)`                                           |
| L2-F  | unknown reservation id                       | `confirm`/`cancel`                          | `Err(ReservationNotFound)`                                                   |
| L2-G  | mix of active reservations, some past `expires_at` | `expire_reservations(now)`             | returns count of newly-expired; each transitioned reservation has state Expired and `expired_at == now`; their stock is released |
| L2-H  | seeded product, after a Confirmed and an Active reservation | `get_available_stock`              | returns `total - confirmed - active`                                         |

## Invariants
- `available_stock = total_stock - confirmed_count - active_reservation_count >= 0` at all times.
- Terminal states (Confirmed, Cancelled, Expired) cannot transition again.
- Stock release happens exactly once per reservation (no double-release across cancel/expire/confirm-after-expiry).

## Out of scope (deferred to L3)
- Concurrent correctness across many threads (locking is currently coarse).
- Background timer / scheduler — `expire_reservations(now)` is invoked by the test/admin caller; a sweeper task is L3 / interface concern.

## Design notes
- Reservations are stored in a second map keyed by `reservation_id`, alongside the per-product counters. The counters are the source of truth for available stock; the reservation map is the source of truth for lifecycle state.
- Time is passed in by the caller (no `Clock` trait yet — we already inject `now`, which keeps L2 tests deterministic without extra abstraction). A `Clock` trait will be introduced when the HTTP layer needs a default `now`.
- Confirm-after-expiry returns `ReservationExpired` *and* mutates state to Expired so callers cannot ride a stale Active reservation by retrying.
