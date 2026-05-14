//! Contention benchmarks for S4 of PLAN.md.
//!
//! Run: `cargo bench`
//! Results land in `target/criterion/` as HTML reports.
//!
//! Two scenarios are measured:
//!
//! ## hot_sku
//! All threads race to reserve from **one product**. The per-product
//! `parking_lot::Mutex` serialises every call, so throughput is bounded by
//! single-lock acquisition rate. This is the worst case for the current design.
//!
//! ## multi_sku
//! Threads are spread evenly across **many independent products**. Each product
//! has its own lock, so concurrent reserves on different SKUs do not block each
//! other. Throughput should grow near-linearly with SKU count.
//!
//! ## When to consider a different concurrency design
//!
//! The current per-product `Mutex` design is appropriate when:
//! - Most contention is between *different* SKUs (multi-SKU workload).
//! - The hot path is CPU-bound, not I/O-bound.
//!
//! A redesign would be justified when:
//! 1. **Single-SKU flash sales dominate** and profiling shows the per-product
//!    mutex as a bottleneck exceeding the target SLA (e.g., > 1 ms p99 at
//!    10k req/sec on one SKU).
//! 2. **Reads vastly outnumber writes**: switching to `RwLock` would allow
//!    parallel `get_available_stock` / `get_reservation` calls while still
//!    serialising mutations.
//! 3. **Atomic CAS**: for a single integer counter (stock only, no reservation
//!    map), a lock-free `AtomicI64` CAS loop eliminates all mutex overhead.
//!    This trades reservation-level consistency for raw throughput and requires
//!    careful design to avoid ABA problems on a full reservation map.
//! 4. **Partitioned queues / actor model**: route each SKU to a dedicated
//!    single-threaded worker; removes locking entirely at the cost of an async
//!    dispatch layer and higher latency for cross-SKU operations.
//!
//! Compare `cargo run --release --bin stress` (single-SKU, 1000 threads) with
//! `cargo bench` (statistical, both scenarios) to quantify the gap and decide
//! whether a redesign is warranted for the target workload.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use chrono::Utc;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use inventory_reservation::{ReservationError, ReservationService};

// ---------------------------------------------------------------------------
// Hot-SKU: all threads race for one product
// ---------------------------------------------------------------------------

fn bench_hot_sku(c: &mut Criterion) {
    let mut group = c.benchmark_group("hot_sku");
    // stock = threads / 2 so ~50 % succeed — exercises both fast (Ok) and
    // contention-rejection (OutOfStock) paths under the same lock pressure.
    for &threads in &[10usize, 50, 200] {
        let stock = (threads / 2) as u32;
        group.throughput(Throughput::Elements(threads as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(threads),
            &threads,
            |b, &threads| {
                b.iter(|| {
                    let svc = Arc::new(ReservationService::new());
                    svc.seed_product("sku-hot", stock);

                    let accepted = Arc::new(AtomicUsize::new(0));
                    let rejected = Arc::new(AtomicUsize::new(0));

                    thread::scope(|scope| {
                        for i in 0..threads {
                            let svc = Arc::clone(&svc);
                            let accepted = Arc::clone(&accepted);
                            let rejected = Arc::clone(&rejected);
                            scope.spawn(move || {
                                match svc.reserve_item(
                                    "sku-hot",
                                    &format!("u{i}"),
                                    Utc::now(),
                                ) {
                                    Ok(_) => {
                                        accepted.fetch_add(1, Ordering::Relaxed);
                                    }
                                    Err(ReservationError::OutOfStock) => {
                                        rejected.fetch_add(1, Ordering::Relaxed);
                                    }
                                    Err(e) => panic!("unexpected: {e:?}"),
                                }
                            });
                        }
                    });

                    // Verify invariant: accepted never exceeds stock.
                    assert!(accepted.load(Ordering::Relaxed) as u32 <= stock);
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Multi-SKU: threads distributed across independent products
// ---------------------------------------------------------------------------

fn bench_multi_sku(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_sku");
    // 10 threads per SKU; each SKU has enough stock so nobody is rejected.
    // Measures parallel throughput across independent per-product locks.
    for &skus in &[5usize, 20, 50] {
        let threads = skus * 10;
        group.throughput(Throughput::Elements(threads as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(skus),
            &skus,
            |b, &skus| {
                b.iter(|| {
                    let svc = Arc::new(ReservationService::new());
                    for i in 0..skus {
                        svc.seed_product(&format!("sku-{i}"), 1000);
                    }

                    thread::scope(|scope| {
                        for t in 0..threads {
                            let svc = Arc::clone(&svc);
                            scope.spawn(move || {
                                let sku = format!("sku-{}", t % skus);
                                let _ = svc.reserve_item(&sku, &format!("u{t}"), Utc::now());
                            });
                        }
                    });
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_hot_sku, bench_multi_sku);
criterion_main!(benches);
