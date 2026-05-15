# Inventory Reservation System (Rust)

Prevents overselling during high-concurrency flash sales. The service holds inventory state in memory, exposes a small reservation API, and guarantees that at most one user wins the last unit even under hundreds of simultaneous requests.

Engineering conventions for contributors (human or AI) are documented in [AGENTS.md](AGENTS.md). The high-level plan lives in [docs/prompt.md](docs/prompt.md).

## Repo Metadata

- Repo start date: 2026-04-25
- Related tech stack versions: Rust stable

## Why This Repo Matters

This repo matters because it models the same reservation domain in Rust, showing ownership, explicit errors, and correctness-focused service design.

## Quickstart

```sh
cargo build
cargo test                            # all levels + reseed + clock + proptest + doctest
cargo test --release                  # exercises the 500-thread stress suite under release
cargo run --release --bin stress      # smoke run: 1000 threads racing for 100 stock
```

Optional gates:

```sh
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo cov-lib                         # library coverage summary (requires cargo-llvm-cov)
cargo doc --no-deps --open            # crate-level rustdoc
```

For follow-up work and compact references, see [PLAN.md](PLAN.md) and [CHEATSHEET.md](CHEATSHEET.md).

## Test coverage

Library coverage (excluding the smoke binary `src/bin/stress.rs`):

| File | Lines | Functions | Regions |
|---|---|---|---|
| `application/reservation_service.rs` | 100% | 100% | 97.83% |
| `domain/clock.rs` | 100% | 100% | 100% |
| **Total** | **100%** | **100%** | **97.85%** |

The remaining ~2% regions are defensive `?` paths after re-fetch under the same lock — unreachable in practice, kept for borrow-checker reasons.

## Performance

Single SKU, no I/O, on a developer laptop:

```
threads:        1000
stock:          100
accepted:       100
out_of_stock:   900
available_left: 0
elapsed:        ~22 ms
throughput:     ~46 000 reserves/sec
```

Reproduce with `cargo run --release --bin stress`.

## Architecture

```text
Test harness  /  src/bin/stress.rs            (only "interface" today)
  -> Application Layer
       ReservationService                     src/application/reservation_service.rs
  -> Domain Layer
       Reservation, ReservationState,
       ReservationError, Clock, SystemClock   src/domain/{reservation,errors,clock}.rs
  -> In-process state (owned by ReservationService)
       DashMap<ProductId, Arc<Mutex<ProductState>>>
       DashMap<ReservationId, ProductId>      (routing index)
```

Per-product fine-grained locks let independent SKUs reserve in parallel, while a per-product `parking_lot::Mutex` makes the read-check-write sequence atomic for any single SKU. **At most one product mutex is held at any instant**, so there is no deadlock surface.

`infrastructure/` and `interface/` modules will appear when an HTTP/gRPC layer or persistent repository is introduced. Adding them now would be empty scaffolding.

## Spec → Test → Code Traceability

| Level / Concern | Spec | Tests | Code |
|---|---|---|---|
| L1 — Basic reservation | [specs/level-1-basic-reservation.md](specs/level-1-basic-reservation.md) | [tests/level_1_basic_reservation.rs](tests/level_1_basic_reservation.rs) | [src/application/reservation_service.rs](src/application/reservation_service.rs) |
| L2 — Lifecycle & expiry | [specs/level-2-lifecycle.md](specs/level-2-lifecycle.md) | [tests/level_2_lifecycle.rs](tests/level_2_lifecycle.rs) | same |
| L3 — Concurrency | [specs/level-3-concurrency.md](specs/level-3-concurrency.md) | [tests/level_3_concurrency.rs](tests/level_3_concurrency.rs) | same |
| Re-seed hygiene | (inline in source) | [tests/reseed.rs](tests/reseed.rs) | same |
| Clock seam | (inline in source) | [tests/clock.rs](tests/clock.rs) | [src/domain/clock.rs](src/domain/clock.rs) |
| Property-based invariants | (derived from L2 + L3) | [tests/proptest_lifecycle.rs](tests/proptest_lifecycle.rs) | same |
| Crate-level doctest | — | [src/lib.rs](src/lib.rs) (`cargo test --doc`) | same |

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
- **Hardening**
  - [x] `Clock` trait + `SystemClock` (time seam without disturbing the per-call `now` API)
  - [x] Re-seed clears stale `reservation_index` entries (no leak across product generations)
  - [x] Property-based fuzz over arbitrary reserve/confirm/cancel/expire sequences (`proptest`)
  - [x] Crate-level runnable doctest
  - [x] Stress smoke binary (`src/bin/stress.rs`) for reviewer-side eyeballing
  - [x] `rust-toolchain.toml` pinned for reproducibility
  - [x] 100% line + function coverage on the library

## Design Decisions and Trade-offs

- **Per-product `parking_lot::Mutex` (chosen)** — short critical section (counter math + map insert), simple to audit, no `.await` so async wrapping is trivial later. `parking_lot` is used over `std::sync::Mutex` to keep library code free of lock-poisoning recovery and `expect()`. **Trade-off:** still serializes within a single hot SKU; for a pure flash-sale on one product, that is exactly what we want.
- **Atomic `compare_exchange` (rejected for now)** — avoids any lock but couples the counter encoding to a single integer and complicates lifecycle transitions where two counters move together (`active--`, `confirmed++`). The mutex version is clearer at the same correctness level for the workload we are tested against.
- **Single-writer actor per SKU (rejected for now)** — would need an async runtime and a channel per product just to gain backpressure benefits we don't need at the in-memory layer. Worth revisiting if a real I/O persistence layer is added.
- **Reservations co-located inside their product (chosen)** — each reservation is guarded by exactly one lock. `confirm`/`cancel` never need two locks, so deadlock cannot occur. **Trade-off:** sweeping all expired reservations is O(products × reservations-per-product), but each product is processed under only its own lock, so SKUs sweep in parallel.
- **Reservation index `DashMap<ReservationId, ProductId>` (chosen)** — gives O(1) routing for `confirm`/`cancel`/`get_reservation` without a global mutex. Re-seeding a product clears stale entries to avoid cross-generation leaks.
- **`Clock` trait + per-call `now` (chosen)** — every public method still accepts `now: DateTime<Utc>` so tests stay deterministic without globals. The `Clock` trait + `SystemClock` are an additional seam for callers (e.g. a future HTTP handler) that prefer to delegate "what time is it?" instead of threading `now` everywhere.
- **Sync API surface (chosen)** — at the in-memory layer there is no I/O to await. Async wrappers will be added at the HTTP boundary; the rule "never hold a mutex guard across `.await`" is documented in [AGENTS.md](AGENTS.md).

## TDD Flow

Commits show the red → green → refactor sequence followed by hardening:

```
test(l1):    RED — failing tests for basic reservation
feat(l1):    GREEN — basic reservation service
test(l2):    RED — failing tests for lifecycle & expiry
feat(l2):    GREEN — reservation lifecycle & expiry
test(l3):    concurrency invariants for high-contention reserve
refactor(l3):per-product locking with DashMap
docs:        README with quickstart, architecture, traceability, AI disclosure
docs(agents):engineering conventions for contributors and AI
chore:       pin rust-toolchain to stable with clippy + rustfmt
docs(lib):   crate-level doctest showing reserve happy + rejected paths
refactor(seed): clear stale reservation_index on re-seed (+ tests/reseed.rs)
feat(bin):   stress smoke binary for parallel reserves
feat(clock): Clock trait with SystemClock default (+ tests/clock.rs)
test:        proptest invariants for reservation lifecycle
docs(prompt):reconcile plan with as-built implementation
test(default): cover Default::default() to reach 100% library coverage
chore(cargo):cov / cov-lib / cov-html aliases
```

L3 has no organic RED phase: the L1/L2 mutex design already enforced the concurrency invariants. The L3 tests are written as executable invariants so any future refactor (such as the per-product locking change) must keep them green.

## AI Usage Disclosure

- **Tool used:** GitHub Copilot in agent mode (Claude Opus 4.7), VS Code Insiders.
- **Where AI helped most:**
  - Spec drafting (markdown tables of behaviour and invariants).
  - Test scaffolding (boilerplate per acceptance criterion).
  - Migrating idioms and tooling notes from a parallel Go version of the same plan to Rust.
  - Property-based test scaffolding (`proptest` strategy + in-test model).
- **Reviewed/rewritten by hand:**
  - Domain rules and invariants (especially confirm-after-expiry, which transitions to Expired before returning `ReservationExpired` so retries cannot ride a stale Active state).
  - Lock discipline and the "at most one product mutex held at any instant" property.
  - Removal of every `unwrap()`/`expect()` from library code by restructuring borrows rather than papering over with messages.
  - Re-seed hygiene (clearing the routing index across product generations).
- **Rejected suggestions:**
  - Using `tokio::sync::Mutex` and an async API for an entirely in-memory state machine (no I/O to await; would just add `Send + 'static` noise).
  - Introducing a `Repository` trait at the L1 stage (no second implementation to justify the abstraction yet).
  - `unsafe` or atomics tricks for the reserve counter (no measured performance need; the mutex version is auditable and already passes the 500-thread test in microseconds).
- **Independent verification:**
  - `cargo test` (debug) and `cargo test --release` both green on every commit at HEAD.
  - `cargo clippy --all-targets -- -D warnings` clean.
  - `cargo fmt --all -- --check` clean.
  - L3-C runs the single-winner contention test 20× per invocation as a smoke check against flaky races.
  - `cargo cov-lib` reports 100% line + function coverage on the library.
  - `grep -nE "\.unwrap\(|\.expect\(" src/` shows zero hits in production code (the only match is in the doctest example).

## Pre-submission checklist

- [x] `cargo build` succeeds on a fresh clone.
- [x] `cargo test` is green (Level 1/2/3 + reseed + clock + proptest + doctest).
- [x] `cargo test --release` stress scenario is green and repeatable.
- [x] `cargo clippy --all-targets -- -D warnings` is clean.
- [x] `cargo fmt --all -- --check` is clean.
- [x] `cargo cov-lib` reports 100% lines / 100% functions on the library.
- [ ] CI workflow (`.github/workflows/ci.yml`) — **intentionally deferred**; the same checks are runnable locally via the commands above.
- [x] Public APIs have purpose-driven names; no commented-out code; no stray `dbg!`/`println!`.
- [x] No `unwrap()`/`expect()` in library/production code paths (only inside the doctest).
- [x] No secrets, tokens, or personal paths committed.
- [x] AI Usage Disclosure section is present and accurate.
