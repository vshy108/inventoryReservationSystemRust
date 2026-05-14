# Inventory Reservation System Rust Improvement Plan

This plan records future work for the Rust reservation service. Follow `AGENTS.md`: spec first, red-green-refactor, signed commits, and no auto-push without reviewer approval.

## S1 — CI Workflow

- [x] Add `.github/workflows/ci.yml` for the same local gates documented in the README.
- [x] Run `cargo build`, `cargo test`, `cargo test --release`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --all -- --check`.
- [x] Keep the intentionally deferred README checklist item in sync with the actual workflow.

## S2 — HTTP Adapter Spec

- [ ] Add a spec for an optional HTTP boundary that preserves the sync core and validates all input at the edge.
- [ ] Define DTOs, error mapping, and endpoint semantics before adding dependencies.
- [ ] Verify first with failing adapter tests, then minimal implementation.

## S3 — Reservation Trace Events

- [ ] Define domain-level trace events for reserve, confirm, cancel, expire, and rejection paths.
- [ ] Keep event collection behind a small port so the core does not depend on logging or I/O.
- [ ] Verify event order and payloads with public service tests.

## S4 — Contention Benchmark

- [ ] Add a repeatable benchmark or stress report for hot-SKU and multi-SKU contention.
- [ ] Compare debug, release, and stress binary results without adding unsafe or atomics by default.
- [ ] Document when a different concurrency design would be justified.

## S5 — Persistence ADR

- [ ] Decide whether a durable adapter belongs in this repo or should remain out of scope.
- [ ] Capture required invariants for reloading products, reservations, and the routing index.
- [ ] Add tests before any adapter implementation.
