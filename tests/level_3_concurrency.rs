//! Level 3 — Concurrency Handling
//!
//! Spec: ../specs/level-3-concurrency.md

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use chrono::{TimeZone, Utc};
use inventory_reservation::{ReservationError, ReservationService};

const THREADS: usize = 500;

fn run_parallel_reserve(svc: Arc<ReservationService>, product_id: &'static str) -> (usize, usize) {
    let successes = Arc::new(AtomicUsize::new(0));
    let oos = Arc::new(AtomicUsize::new(0));
    let now = Utc.timestamp_opt(1_000, 0).single().expect("valid ts");

    thread::scope(|s| {
        for tid in 0..THREADS {
            let svc = Arc::clone(&svc);
            let successes = Arc::clone(&successes);
            let oos = Arc::clone(&oos);
            s.spawn(move || {
                let user = format!("user-{tid}");
                match svc.reserve_item(product_id, &user, now) {
                    Ok(_) => {
                        successes.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(ReservationError::OutOfStock) => {
                        oos.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(other) => panic!("unexpected error under contention: {other:?}"),
                }
            });
        }
    });

    (
        successes.load(Ordering::Relaxed),
        oos.load(Ordering::Relaxed),
    )
}

// --- L3-A: 500 threads, stock = 1 -> exactly 1 winner ---------------------------
#[test]
fn l3_a_single_unit_single_winner() {
    let svc = Arc::new(ReservationService::new());
    svc.seed_product("sku-1", 1);

    let (ok, oos) = run_parallel_reserve(Arc::clone(&svc), "sku-1");

    assert_eq!(ok, 1, "exactly one reservation must succeed");
    assert_eq!(oos, THREADS - 1, "all others must be OutOfStock");
    assert_eq!(svc.get_available_stock("sku-1").unwrap(), 0);
}

// --- L3-B: stock = N, 500 threads -> exactly N winners --------------------------
#[test]
fn l3_b_n_units_n_winners() {
    const N: u32 = 50;
    let svc = Arc::new(ReservationService::new());
    svc.seed_product("sku-1", N);

    let (ok, oos) = run_parallel_reserve(Arc::clone(&svc), "sku-1");

    assert_eq!(ok, N as usize);
    assert_eq!(oos, THREADS - N as usize);
    assert_eq!(svc.get_available_stock("sku-1").unwrap(), 0);
}

// --- L3-C: repeat L3-A many times -> deterministic outcome ----------------------
#[test]
fn l3_c_single_winner_is_deterministic_across_runs() {
    for run in 0..20 {
        let svc = Arc::new(ReservationService::new());
        svc.seed_product("sku-1", 1);

        let (ok, oos) = run_parallel_reserve(Arc::clone(&svc), "sku-1");

        assert_eq!(ok, 1, "run {run}: expected exactly one winner");
        assert_eq!(oos, THREADS - 1, "run {run}: expected 499 rejections");
    }
}

// --- L3-D: independent SKUs do not interfere ------------------------------------
#[test]
fn l3_d_multi_product_no_interference() {
    let svc = Arc::new(ReservationService::new());
    svc.seed_product("sku-a", 1);
    svc.seed_product("sku-b", 1);
    svc.seed_product("sku-c", 1);

    let now = Utc.timestamp_opt(1_000, 0).single().expect("valid ts");
    let products = ["sku-a", "sku-b", "sku-c"];

    let counters: Vec<Arc<AtomicUsize>> = products
        .iter()
        .map(|_| Arc::new(AtomicUsize::new(0)))
        .collect();

    thread::scope(|s| {
        for tid in 0..THREADS {
            let svc = Arc::clone(&svc);
            let product_idx = tid % products.len();
            let product = products[product_idx];
            let ok_counter = Arc::clone(&counters[product_idx]);
            s.spawn(move || {
                let user = format!("user-{tid}");
                if svc.reserve_item(product, &user, now).is_ok() {
                    ok_counter.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });

    for (i, product) in products.iter().enumerate() {
        let ok = counters[i].load(Ordering::Relaxed);
        assert_eq!(ok, 1, "{product} must have exactly one winner");
        assert_eq!(svc.get_available_stock(product).unwrap(), 0);
    }
}
