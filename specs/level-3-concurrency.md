# Spec — Level 3: Concurrency Handling

Drives: [tests/level_3_concurrency.rs](../tests/level_3_concurrency.rs)

## Goal
Guarantee that under high contention the service never oversells, never underflows counters, and never wedges.

## Behaviour

| ID    | Given                                          | When                                              | Then                                                                |
|-------|------------------------------------------------|---------------------------------------------------|---------------------------------------------------------------------|
| L3-A  | `total_stock = 1`                              | 500 OS threads simultaneously call `reserve_item` | exactly 1 returns Ok; the other 499 return `Err(OutOfStock)`        |
| L3-B  | `total_stock = N` (e.g. 50), 500 threads        | each thread calls `reserve_item` once             | exactly N return Ok; remaining `500 - N` return `Err(OutOfStock)`   |
| L3-C  | `total_stock = 1`, repeat L3-A 20 times        | parallel reservations on a fresh service per run  | every run shows exactly 1 success and 499 failures (deterministic)  |
| L3-D  | mixed workload across multiple products        | parallel reserves across SKUs                     | no SKU oversells; counters and reservation map remain consistent    |

## Invariants under contention
- `available_stock >= 0` at all observable points.
- `successes(product) <= total_stock(product)`.
- `successes(product) + active_reservation_count(product)` is consistent (i.e. each `Ok` is reflected in the state).
- No deadlocks within a per-test wall-clock budget (e.g. 5s for 500 threads on stock = 1).
- No panics (clippy/Miri-style: counter math stays non-negative).

## Out of scope
- Tuning for maximum parallel reserves on different SKUs (that is the per-product locking refactor — separate optional step).
- `loom` model checking (optional, gated behind a feature flag if added later).

## Design notes
- Concurrency primitive: per-product `parking_lot::Mutex<ProductState>` guards backed by a `DashMap<ProductId, Arc<Mutex<ProductState>>>`. This was the chosen final design (delivered in `refactor(l3): per-product locking with DashMap`), allowing parallel reserves across different SKUs while serializing the check-then-act for any single SKU. At most one product mutex is held at any instant, so the deadlock surface is empty by construction.
- Tests run as `#[ignore]`-free integration tests so a default `cargo test` exercises them. They use `std::thread::scope` (no async runtime needed), and assert both the count of successes and the post-hoc state of the service.
- The repeat-test (L3-C) defends against flaky outcomes that would indicate races slipping through.
