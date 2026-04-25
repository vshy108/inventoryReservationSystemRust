# Plan — Inventory Reservation System (Flash Sale Concurrency Challenge)

## 1. Problem Summary
Build an **Inventory Reservation System** that prevents overselling during high-concurrency flash sales.

Language target: **Rust** (stable toolchain), with in-memory state and explicit concurrency control.

Core scenario from the challenge:
- Stock can be very limited (for example, 1 item).
- Many users can try to reserve the same item at nearly the same time (for example, 500 requests).
- The system must allow at most one successful reservation for the last item and reject the rest.

Primary objectives:
- Prevent overselling.
- Handle concurrent requests safely.
- Manage temporary reservations with expiry.
- Maintain a consistent inventory state at all times.

---

## 2. Business Rules
- Available stock formula:
  - **Available Stock = Total Stock - Confirmed Sales - Active Reservations**
- Reservation requests that exceed available stock must fail.
- Confirmed purchases are final and cannot be reversed.
- Only one user can reserve the last item.
- Reservations automatically release inventory when expired.
- Reservation hold time: **2 minutes**.

---

## 3. Deliverables by Level

### Level 1 — Basic Inventory Reservation
- Keep inventory state in memory.
- Implement `reserve_item(product_id, user_id) -> Result<Reservation, ReservationError>`.
- Reject reservation when no stock is available.
- Example acceptance check:
  - Stock = 1
  - User A reserve -> success
  - User B reserve -> fail

### Level 2 — Reservation Lifecycle & Expiry
- Add reservation states (Rust `enum ReservationState`):
  - `Active`
  - `Confirmed`
  - `Cancelled`
  - `Expired`
- Add `confirm_reservation(reservation_id)` transition:
  - Converts an active hold into a confirmed sale.
- Add `cancel_reservation(reservation_id)` transition:
  - Releases reserved inventory.
- Implement automatic expiry (2-minute hold):
  - Expired reservations release inventory.

### Level 3 — Concurrency Handling
- Prevent race conditions under simultaneous requests.
- Validate with scenario:
  - Stock = 1
  - Simultaneous requests = 500
  - Expected: 1 success, 499 failures
- Use an explicit thread-safety strategy:
  - Per-product `Mutex` / `RwLock` (e.g., backed by `DashMap<ProductId, Arc<Mutex<...>>>`), or
  - Atomic transactional update with optimistic retry (`compare_exchange` on `AtomicU32`), or
  - Single-writer task per SKU using a `tokio::sync::mpsc` channel.

---

## 4. Domain Model

```rust
// Internal to ReservationService; not a public type.
struct ProductState {
    total_stock: u32,
    confirmed_count: u32,
    active_reservation_count: u32,
    reservations: HashMap<String, Reservation>,
}

pub enum ReservationState {
    Active,
    Confirmed,
    Cancelled,
    Expired,
}

pub struct Reservation {
    pub reservation_id: String,
    pub product_id: String,
    pub user_id: String,
    pub state: ReservationState,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
}
```

> Event sourcing (`EventType` / `InventoryEvent`) is intentionally out of scope for this challenge — counts and reservation state are sufficient to satisfy the spec. Add events only when an audit/observability requirement materialises.

### Invariants
- `available_stock >= 0` must always hold (use unsigned counters and saturating/checked arithmetic).
- `active_reservation_count >= 0` and `confirmed_count >= 0` (enforced by `u32`).
- A reservation can only move through valid transitions:
  - `Active -> Confirmed | Cancelled | Expired`
  - Terminal states (`Confirmed`, `Cancelled`, `Expired`) cannot transition.
- Confirm action only valid for `Active` reservation before expiry.

---

## 5. Rust Service API

The core is **synchronous** — the in-memory critical section is short and
bounded, so async machinery would only add complexity. Async will be
introduced at the edge (HTTP/gRPC handler) when that lands.

```rust
impl ReservationService {
    pub fn new() -> Self;
    pub fn seed_product(&self, product_id: &str, total_stock: u32);
    pub fn reserve_item(&self, product_id: &str, user_id: &str, now: DateTime<Utc>)
        -> Result<Reservation, ReservationError>;
    pub fn confirm_reservation(&self, reservation_id: &str, now: DateTime<Utc>)
        -> Result<Reservation, ReservationError>;
    pub fn cancel_reservation(&self, reservation_id: &str, now: DateTime<Utc>)
        -> Result<Reservation, ReservationError>;
    pub fn expire_reservations(&self, now: DateTime<Utc>) -> Result<usize, ReservationError>;
    pub fn get_available_stock(&self, product_id: &str) -> Result<u32, ReservationError>;
    pub fn get_reservation(&self, reservation_id: &str) -> Option<Reservation>;
}
```

There is intentionally no `trait ReservationService` because the codebase
has exactly one implementation (per AGENTS.md "don't add a trait when
there is one impl"). A `Clock` trait *is* provided for the time seam.

Error model with `thiserror`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ReservationError {
    #[error("out of stock")]
    OutOfStock,
    #[error("reservation not found")]
    ReservationNotFound,
    #[error("reservation already finalized")]
    ReservationAlreadyFinalized,
    #[error("reservation expired")]
    ReservationExpired,
    #[error("product not found")]
    ProductNotFound,
}
```

---

## 6. Architecture (Clean and Testable)

```text
Test harness  /  src/bin/stress.rs   (the only "interface" today)
  -> Application Layer (ReservationService)
    -> Domain Layer (Reservation, ReservationState, ReservationError, Clock)
```

Key design points:
- Keep domain logic deterministic and side-effect free.
- Inject `now: DateTime<Utc>` per call so tests pin time without globals;
  a `Clock` trait + `SystemClock` exists for callers that prefer
  delegation.
- Concurrency is centralised in `ReservationService` via per-product
  `parking_lot::Mutex` guards — one product mutex held at any instant.
- Run `cargo test` plus stress runs under `--release` to validate thread safety.

---

## 7. Concurrency Strategy (As Built)

- `DashMap<ProductId, Arc<parking_lot::Mutex<ProductState>>>` provides
  per-product locking. Different SKUs scale linearly.
- Inside the lock:
  1. Recompute available stock (`total - confirmed - active`).
  2. If available == 0, reject with `OutOfStock`.
  3. Allocate the reservation and increment `active_reservation_count`.
- A separate `DashMap<ReservationId, ProductId>` routes
  `confirm_reservation` / `cancel_reservation` to the owning product
  without scanning.
- The library is sync; `parking_lot::Mutex` does not poison and never
  needs `expect()` on lock acquisition.
- At most **one product mutex** is held at any instant — the deadlock
  surface is empty by construction.

Not done (deliberately):
- No atomic CAS / lock-free path: the lock is not the bottleneck at this
  scale (the stress binary measures ~40k reserves/sec on a single SKU).
- No `loom` / `miri`: see AGENTS.md.

---

## 8. TDD Plan

### Level 1 tests
- Reserve succeeds when stock available.
- Reserve fails when stock unavailable.
- Available stock calculation follows business formula.

### Level 2 tests
- Confirm active reservation updates counts and state.
- Cancel active reservation releases stock and sets state.
- Expiry job transitions timed-out active reservations to expired.
- Confirm after expiry fails.

### Level 3 tests
- 500 parallel reserve calls for stock 1 -> exactly 1 success (using `std::thread::scope`).
- No underflow or inconsistent counters under contention.
- Repeated high-concurrency runs remain deterministic in outcome counts.
- `proptest` invariants over arbitrary reserve/confirm/cancel/expire
  sequences, asserting `available == total - active - confirmed` after
  every step (`tests/proptest_lifecycle.rs`).

---

## 9. Project Layout (As Built)

```text
inventoryReservationSystemRust/
├── AGENTS.md
├── Cargo.toml
├── Cargo.lock
├── README.md
├── rust-toolchain.toml
├── docs/
│   └── prompt.md
├── specs/
│   ├── level-1-basic-reservation.md
│   ├── level-2-lifecycle.md
│   └── level-3-concurrency.md
├── src/
│   ├── lib.rs
│   ├── bin/
│   │   └── stress.rs                     # smoke binary
│   ├── application/
│   │   ├── mod.rs
│   │   └── reservation_service.rs
│   └── domain/
│       ├── mod.rs
│       ├── clock.rs
│       ├── errors.rs
│       └── reservation.rs
└── tests/
    ├── level_1_basic_reservation.rs
    ├── level_2_lifecycle.rs
    ├── level_3_concurrency.rs
    ├── reseed.rs
    ├── clock.rs
    └── proptest_lifecycle.rs
```

`infrastructure/` and `interface/` modules will appear when an HTTP/gRPC
layer or persistent repository is introduced. Adding them now would be
empty scaffolding.

---

## 10. Implementation Milestones
1. Bootstrap Cargo project (`cargo init` or `cargo new --lib`), add baseline tests, and one passing smoke test.
2. Implement Level 1 reservation + stock formula tests.
3. Implement Level 2 lifecycle transitions + expiry handling.
4. Implement Level 3 locking strategy + parallel stress test.
5. Add README and architecture notes, then final verification with `cargo test` (debug + release) and `cargo clippy -- -D warnings`.

---

## 11. Evaluation Alignment Checklist
- Correctness:
  - No overselling.
  - Reservation lifecycle rules are respected.
- Concurrency handling:
  - 500 concurrent requests on stock 1 produce exactly 1 success.
- Expiry logic:
  - Timed-out active reservations release stock automatically.
- Code quality:
  - Clear layering, explicit error model (`thiserror`), deterministic tests.
  - Locking strategy is documented and simple to audit.
  - Idiomatic Rust: no needless `clone()`, no `unwrap()` in library code, ownership boundaries are obvious.

---

## 12. Done Criteria
- All Level 1, 2, and 3 acceptance tests pass.
- Concurrency scenario is automated and repeatable.
- Stock formula holds under all transitions and stress runs.
- Documentation clearly explains lifecycle and locking decisions.
- A reviewer can clone, run tests, and verify in under a few minutes.
- Verification commands are documented and green:
  - `cargo build --release`
  - `cargo test`
  - `cargo test --release` (for the stress scenario)
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo fmt --all -- --check`

---

## 13. Submission Requirements (from challenge email, adapted for Rust)
The reviewer expects a ZIP (or Drive link) and will judge the code
**as if written by an experienced engineer**. AI usage is allowed and
expected, but does not lower the quality bar.

### Explicit grading values
- **SOLID principles**: apply where they reduce coupling; avoid over-engineering.
- **Test-driven development**: red -> green -> refactor should be visible in commit flow.
- **Clean, readable code**: small focused functions, clear names, no dead code, no `unwrap()`/`expect()` outside tests.
- **OOP and design patterns where appropriate**: in Rust, favor traits at module boundaries,
  composition via embedding/generics over inheritance, and lightweight patterns
  (Repository, Policy, typed Result errors via `thiserror`) only when they improve clarity.
- **Reviewer DX**: make the solution easy to run, inspect, and validate quickly.

### Mandatory deliverables in the ZIP
- `README.md` with:
  - One-paragraph problem statement.
  - **Quickstart** (<= 3 commands):
    - `cargo build`
    - `cargo test` (includes Level 1/2/3 + reseed + clock + proptest + doctest)
    - `cargo run --release --bin stress` (visual stress smoke)
  - Architecture diagram (layered ASCII block is enough).
  - Spec -> test -> code traceability table (if using a `specs/` folder).
  - Level 1 / 2 / 3 feature checklist with status.
  - **Design decisions and trade-offs** section (lock vs. atomic vs. actor).
  - **AI usage disclosure** section (see below).
- `AGENTS.md` (if present in the repo) for engineering conventions.
- `specs/` directory (if used) that drives implementation.
- `Cargo.toml` and `Cargo.lock` committed (lockfile committed because this repo ships a binary).
- `rust-toolchain.toml` pinning the toolchain (`stable` + `clippy`/`rustfmt`).
- CI workflow (`.github/workflows/ci.yml`) is **intentionally deferred** — the
  same checks are runnable locally via the verification commands in §12.
- Clean git history with conventional commits where practical.
- Exclude build artifacts from the ZIP (`target/`, coverage output, temporary logs).

### AI usage disclosure (recommended)
Add a section to `README.md` titled `## AI Usage Disclosure` covering:
- Tools used (for example, GitHub Copilot in agent mode; model name if desired).
- Where AI helped most (spec drafting, test scaffolding, boilerplate).
- What was reviewed or rewritten by hand (domain rules, invariants, service boundaries, unsafe/atomic logic).
- What was rejected and why (over-engineered patterns, unsafe shortcuts).
- Verification done independently of AI (tests, stress runs, clippy, diff review, edge-case checks).

### Pre-submission checklist
- [ ] `cargo build` succeeds on a fresh clone.
- [ ] `cargo test` is green.
- [ ] `cargo test --release` stress scenario is green and repeatable.
- [ ] `cargo clippy --all-targets -- -D warnings` is clean.
- [ ] `cargo fmt --all -- --check` is clean.
- [ ] Public APIs have purpose-driven names; no commented-out code; no stray `dbg!`/`println!`.
- [ ] No `unwrap()`/`expect()` in library/production code paths.
- [ ] No secrets, tokens, or personal paths committed.
- [ ] README quickstart is verified line by line on a clean machine.
- [ ] AI Usage Disclosure section is present and accurate.
- [ ] ZIP excludes `.git/`, `target/`, local caches, and generated artifacts unless explicitly requested.
