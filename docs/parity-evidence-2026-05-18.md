# Rust IRS Parity Evidence - 2026-05-18

This note records a fresh validation pass comparing the Rust inventory reservation implementation against the refreshed Go IRS behavior.

## Goal

Prove the Rust implementation covers the same critical reservation behavior as the Go system: reserve, reject out-of-stock, confirm, cancel, expose stock, preserve lifecycle rules, and remain correct under concurrent reservation attempts.

## Commands Run

```sh
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Results

| Check | Result | Evidence |
|-------|--------|----------|
| Full Rust test suite | Passed | `cargo test` completed successfully |
| Clippy all targets/features | Passed | `cargo clippy --all-targets --all-features -- -D warnings` completed cleanly |
| HTTP boundary parity | Passed | `tests/http_adapter.rs`: 17 tests cover reserve, confirm, cancel, stock, lookup, expire, validation, 404, and conflict mapping |
| Lifecycle parity | Passed | `tests/level_2_lifecycle.rs`: 9 tests cover confirm, cancel, finalized, and expired states |
| Concurrency parity | Passed | `tests/level_3_concurrency.rs`: 4 tests cover contention and oversell prevention |
| Property checks | Passed | `tests/proptest_lifecycle.rs`: lifecycle property test passed |
| Trace/event behavior | Passed | `tests/trace_events.rs`: 10 tests verify event trace behavior |

## Go IRS Comparison

The refreshed Go IRS evidence proves API behavior, Postgres migration safety, and load capacity. The Rust IRS evidence proves the matching domain and HTTP-boundary semantics locally:

| Behavior | Go IRS evidence | Rust IRS evidence |
|----------|-----------------|-------------------|
| Reserve succeeds when stock exists | HTTP smoke and k6 reservation creation | HTTP adapter and basic reservation tests |
| Out-of-stock rejects | HTTP smoke expects `409` | HTTP adapter/domain tests map out-of-stock to conflict |
| Confirm and cancel lifecycle | HTTP smoke confirms and cancels reservations | Lifecycle tests cover confirm, cancel, finalized, and expired states |
| Stock remains correct | HTTP smoke checks stock after reserve/cancel | Stock and concurrency tests prove no oversell |
| Concurrency contention | k6 load test exercises many reservations | Concurrency tests prove at-most-stock winners under simultaneous attempts |

## Reviewer Signal

The Rust implementation has not grown a full persistence/load stack yet, but it proves the same critical domain behavior as the Go system with stronger local correctness checks around lifecycle, event traces, and concurrency.

Next useful slice: choose whether Rust IRS should stay a domain-correctness proof or receive a real persistence/API runtime slice comparable to Go IRS.