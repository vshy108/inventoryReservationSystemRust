# Inventory Reservation System (Rust)

Prevents overselling during high-concurrency flash sales. The service holds inventory state in memory, exposes a small reservation API, and guarantees that at most one user wins the last unit even under hundreds of simultaneous requests.

## Quickstart

```sh
cargo build
cargo test
cargo test --release        # exercises the 500-thread stress suite under release
```

Optional gates (used in CI):

```sh
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Architecture

```text
Interface Layer  (CLI / HTTP — not yet implemented)
  -> Application Layer
       ReservationService          src/application/reservation_service.rs
  -> Domain Layer
       Reservation, ReservationState, ReservationError
                                   src/domain/{reservation,errors}.rs
  -> Infrastructure (in-process)
       DashMap<ProductId, Arc<Mutex<ProductState>>>
       DashMap<ReservationId, ProductId>      (routing index)
```

Per-product fine-grained locks let independent SKUs reserve in parallel, while a per-product `parking_lot::Mutex` makes the read-check-write sequence atomic for any single SKU. At most one product mutex is held at any instant, so there is no deadlock surface.

## Spec → Test → Code Traceability

| Level | Spec                                              | Tests                                              | Code                                                          |
|-------|---------------------------------------------------|----------------------------------------------------|---------------------------------------------------------------|
| L1    | [specs/level-1-basic-reservation.md](specs/level-1-basic-reservation.md) | [tests/level_1_basic_reservation.rs](tests/level_1_basic_reservation.rs) | [src/application/reservation_service.rs](src/application/reservation_service.rs) |
| L2    | [specs/level-2-lifecycle.md](specs/level-2-lifecycle.md)                 | [tests/level_2_lifecycle.rs](tests/level_2_lifecycle.rs)                 | same                                                          |
| L3    | [specs/level-3-concurrency.md](specs/level-3-concurrency.md)             | [tests/level_3_concurrency.rs](tests/level_3_concurrency.rs)             | same                                                          |

## Feature Checklist

- **Level 1 — Basic Reservation**
  - [x] Reserve when stock available
  - [x] Reject when stock = 0 (`OutOfStock`)
  - [x] Reject single-winner contention (`OutOfStock` for the loser)
  - [x] Reject unknown product (`ProductNotFound`)
- **Level 2 — Lifecycle & Expiry**
  - [x] `confirm_reservation` (Active → Confirmed)
  - [x] `cancel_reservation` (Active → Cancelled, releases stock)
  - [x] `expire_reservations(now)` sweep (Active → Expired, releases stock)
  - [x] Confirm-after-expiry returns `ReservationExpired` *and* transitions to Expired
  - [x] Finalized states reject further transitions (`ReservationAlreadyFinalized`)
  - [x] `get_available_stock = total - confirmed - active`
- **Level 3 — Concurrency**
  - [x] 500 threads, stock = 1 → exactly 1 winner
  - [x] 500 threads, stock = N → exactly N winners
  - [x] Repeated stress runs are deterministic
  - [x] Independent SKUs do not interfere

## Design Decisions and Trade-offs

- **Per-product `parking_lot::Mutex` (chosen)** — short critical section (counter math + map insert), simple to audit, no `.await` so async wrapping is trivial later. `parking_lot` is used over `std::sync::Mutex` to keep library code free of lock-poisoning recovery and `expect()`. **Trade-off:** still serializes within a single hot SKU; for a pure flash-sale on one product, that is exactly what we want.
- **Atomic `compare_exchange` (rejected for now)** — avoids any lock but couples the counter encoding to a single integer and complicates lifecycle transitions where two counters move together (`active--`, `confirmed++`). The mutex version is clearer at the same correctness level for the workload we are tested against.
- **Single-writer actor per SKU (rejected for now)** — would need an async runtime and a channel per product just to gain backpressure benefits we don't need at the in-memory layer. Worth revisiting if a real I/O persistence layer is added.
- **Reservations co-located inside their product (chosen)** — each reservation is guarded by exactly one lock. `confirm`/`cancel` never need two locks, so deadlock cannot occur. **Trade-off:** sweeping all expired reservations is O(products × reservations-per-product), but each product is processed under only its own lock, so SKUs sweep in parallel.
- **Reservation index `DashMap<ReservationId, ProductId>` (chosen)** — gives O(1) routing for `confirm`/`cancel`/`get_reservation` without a global mutex.
- **No `Clock` trait yet (chosen)** — `now: DateTime<Utc>` is already injected into every public method, which makes tests deterministic without extra abstraction. A `Clock` trait will land when the HTTP layer needs a default `now`.
- **Sync API surface (chosen)** — the spec keeps `async fn` as the eventual shape, but at the in-memory layer there is no I/O to await. Async wrappers will be added at the HTTP boundary; the rule "never hold a mutex guard across `.await`" is documented in the spec.

## TDD Flow

Commits show the red → green → refactor sequence:

```
test(l1):  RED — failing tests for basic reservation
feat(l1):  GREEN — basic reservation service
test(l2):  RED — failing tests for lifecycle & expiry
feat(l2):  GREEN — reservation lifecycle & expiry
test(l3):  concurrency invariants for high-contention reserve
refactor(l3): per-product locking with DashMap
```

L3 has no organic RED phase: the L1/L2 mutex design already enforced the concurrency invariants. The L3 tests are written as executable invariants so any future refactor (such as the per-product locking change) must keep them green.

## AI Usage Disclosure

- **Tool used:** GitHub Copilot in agent mode (Claude Opus 4.7), VS Code Insiders.
- **Where AI helped most:**
  - Spec drafting (markdown tables of behaviour and invariants).
  - Test scaffolding (boilerplate per acceptance criterion).
  - Migrating idioms and tooling notes from a parallel Go version of the same plan to Rust.
- **Reviewed/rewritten by hand:**
  - Domain rules and invariants (especially confirm-after-expiry, which transitions to Expired before returning `ReservationExpired` so retries cannot ride a stale Active state).
  - Lock discipline and the "at most one product mutex held at any instant" property.
  - Removal of every `unwrap()`/`expect()` from library code by restructuring borrows rather than papering over with messages.
- **Rejected suggestions:**
  - Using `tokio::sync::Mutex` and an async API for an entirely in-memory state machine (no I/O to await; would just add `Send + 'static` noise).
  - Introducing a `Repository` trait at the L1 stage (no second implementation to justify the abstraction yet).
  - `unsafe` or atomics tricks for the reserve counter (no measured performance need; the mutex version is auditable and already passes the 500-thread test in microseconds).
- **Independent verification:**
  - `cargo test` (debug) and `cargo test --release` both green on every commit at HEAD.
  - `cargo clippy --all-targets -- -D warnings` clean.
  - `cargo fmt --all -- --check` clean.
  - L3-C runs the single-winner contention test 20× per invocation as a smoke check against flaky races.
  - `grep -nE "\.unwrap\(|\.expect\(" src/` shows zero hits in production code.

## Pre-submission checklist

- [x] `cargo build` succeeds on a fresh clone.
- [x] `cargo test` is green.
- [x] `cargo test --release` stress scenario is green and repeatable.
- [x] `cargo clippy --all-targets -- -D warnings` is clean.
- [x] `cargo fmt --all -- --check` is clean.
- [ ] CI badge in `README.md` is green (CI workflow added in a follow-up commit).
- [x] Public APIs have purpose-driven names; no commented-out code; no stray `dbg!`/`println!`.
- [x] No `unwrap()`/`expect()` in library/production code paths.
- [x] No secrets, tokens, or personal paths committed.
- [x] AI Usage Disclosure section is present and accurate.
