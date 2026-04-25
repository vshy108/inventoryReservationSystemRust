# Spec — Level 1: Basic Inventory Reservation

Drives: [tests/level_1_basic_reservation.rs](../tests/level_1_basic_reservation.rs)

## Goal
Allow a user to reserve a single unit of a product when stock is available, and reject when it is not. State lives in memory.

## Public API (under test)

```rust
ReservationService::new() -> Self
ReservationService::seed_product(&self, product_id: &str, total_stock: u32)
ReservationService::reserve_item(
    &self,
    product_id: &str,
    user_id: &str,
    now: DateTime<Utc>,
) -> Result<Reservation, ReservationError>
```

## Behaviour

| ID    | Given                                            | When                          | Then                                                              |
|-------|--------------------------------------------------|-------------------------------|-------------------------------------------------------------------|
| L1-A  | a product seeded with `total_stock = 1`          | a user calls `reserve_item`   | returns `Ok(Reservation { state: Active, .. })` for that product+user |
| L1-B  | a product seeded with `total_stock = 0`          | a user calls `reserve_item`   | returns `Err(ReservationError::OutOfStock)`                       |
| L1-C  | `total_stock = 1`, user A already reserved       | user B calls `reserve_item`   | user B returns `Err(OutOfStock)`; user A's reservation unchanged  |
| L1-D  | product was never seeded                         | any user calls `reserve_item` | returns `Err(ReservationError::ProductNotFound)`                  |

## Invariants enforced at L1
- `available_stock = total_stock - active_reservation_count` (no confirmations yet at L1).
- `available_stock >= 0`.
- A successful `reserve_item` returns a reservation with `state == Active`.

## Out of scope (deferred)
- L2: confirm / cancel / expire transitions, expiry timestamp semantics.
- L3: concurrent-safety guarantees and stress test.
- HTTP / async surface, persistence, observability.

## Design notes
- `ReservationService` holds state directly; no repository trait yet (introduced when L3 forces a per-product locking seam).
- Sync API; async boundary added later with the HTTP layer.
- IDs generated with a small dependency-free counter for now; can swap to `uuid` when externally observable.
