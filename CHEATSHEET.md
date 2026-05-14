# Inventory Reservation System Rust Cheatsheet

## Commands

```sh
cargo build
cargo test
cargo test --release
cargo run --release --bin stress
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo cov-lib
cargo doc --no-deps --open
```

## Core Model

- Product state lives in memory.
- Each product has its own `parking_lot::Mutex`.
- At most one product mutex is held at a time.
- `ReservationId -> ProductId` routing uses a `DashMap` index.
- `Clock` is a seam for deterministic time in tests and adapters.

## Reservation States

```text
Active -> Confirmed
Active -> Cancelled
Active -> Expired
```

- Confirmed, cancelled, and expired reservations are final.
- Confirm after expiry transitions the reservation to `Expired` before returning `ReservationExpired`.
- Re-seeding a product clears stale reservation-index entries.

## Stress And Coverage

```sh
cargo test --release
cargo run --release --bin stress
cargo cov-lib
```

Expected stress shape:

```text
threads: 1000
stock: 100
accepted: 100
out_of_stock: 900
available_left: 0
```

## Extension Rules

- Keep the core service synchronous while it is in-memory only.
- Add async at the HTTP or persistence boundary.
- Do not hold a mutex guard across `.await`.
- Avoid `unwrap()` and `expect()` in library or production paths.
- Add a new abstraction only when a second implementation exists.
