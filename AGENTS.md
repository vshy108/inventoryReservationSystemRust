# AGENTS.md — Engineering Conventions

These rules apply to any contributor (human or AI) working in this repository.

## Workflow

- **Spec-driven, TDD.** Add or update a file under `specs/` before changing code. Each spec drives a corresponding `tests/level_*.rs` file.
- **Red → Green → Refactor commits.** A `test(...)` commit must show failing tests; the next `feat(...)` commit makes them pass; refactors are separate `refactor(...)` commits with no test changes.
- **Conventional commits.** `feat`, `fix`, `test`, `refactor`, `docs`, `chore`. Use a level scope when relevant: `feat(l2): ...`.
- **Sign every commit** (`-S`). Commits land on `main` only after `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --all -- --check` are clean.
- **Do not auto-push.** The user reviews and pushes manually.

## Code rules

- **No `unwrap()` / `expect()` in `src/`.** Restructure borrows or return a typed error. `expect()` is allowed in `tests/` and doctests.
- **No `dbg!` / stray `println!` / commented-out code.**
- **Errors are typed (`thiserror`).** Boundary code may map to `anyhow` only at the binary edge.
- **Locks held without `.await`.** Any future async wrapper must release `parking_lot::Mutex` guards before any `.await`.
- **One product mutex held at any instant.** New code must preserve this property to keep the deadlock surface empty.
- **Sync core, async at the edge.** The library is sync; async only enters when an HTTP/gRPC interface lands.

## Tooling

- Toolchain pinned via `rust-toolchain.toml`.
- `cargo` only — no `npm`, `yarn`, or shell scripts driving builds.
- For multi-step terminal work, prefer one command at a time so failures are visible.

## Layout

```
specs/                     # one .md file per level/feature, drives tests
src/
  lib.rs
  domain/                  # entities + typed errors only, no I/O
  application/             # ReservationService and friends
  infrastructure/          # (placeholder) repositories, locks, clock
  interface/               # (placeholder) CLI / HTTP DTOs
tests/                     # one file per spec (level_1_*, level_2_*, level_3_*)
docs/prompt.md             # the high-level plan
```

## What NOT to add without justification

- A `Repository` / `Service` trait when there is exactly one implementation.
- `tokio` or `async_trait` for purely in-memory paths.
- `unsafe`, hand-rolled atomics, or `loom` tests unless a benchmark proves the lock is the bottleneck.
- New crates: each dependency must be justified in the commit body.
