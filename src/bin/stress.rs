//! Stress smoke binary: spawns N threads racing to reserve from a single
//! product, prints timings + accepted/rejected counts, and verifies that
//! invariants hold. Intended as a quick visual sanity check for reviewers.
//!
//! Run: `cargo run --release --bin stress`

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use chrono::Utc;
use inventory_reservation::{ReservationError, ReservationService};

const THREADS: usize = 1_000;
const STOCK: u32 = 100;
const PRODUCT: &str = "sku-stress";

fn main() {
    let svc = Arc::new(ReservationService::new());
    svc.seed_product(PRODUCT, STOCK);

    let accepted = Arc::new(AtomicUsize::new(0));
    let out_of_stock = Arc::new(AtomicUsize::new(0));

    let start = Instant::now();
    thread::scope(|scope| {
        for i in 0..THREADS {
            let svc = Arc::clone(&svc);
            let accepted = Arc::clone(&accepted);
            let out_of_stock = Arc::clone(&out_of_stock);
            scope.spawn(move || {
                let user = format!("user-{i}");
                match svc.reserve_item(PRODUCT, &user, Utc::now()) {
                    Ok(_) => {
                        accepted.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(ReservationError::OutOfStock) => {
                        out_of_stock.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(other) => panic!("unexpected error: {other:?}"),
                }
            });
        }
    });
    let elapsed = start.elapsed();

    let accepted = accepted.load(Ordering::Relaxed);
    let out_of_stock = out_of_stock.load(Ordering::Relaxed);
    let available = match svc.get_available_stock(PRODUCT) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("seeded product missing: {e:?}");
            std::process::exit(1);
        }
    };

    println!("threads:        {THREADS}");
    println!("stock:          {STOCK}");
    println!("accepted:       {accepted}");
    println!("out_of_stock:   {out_of_stock}");
    println!("available_left: {available}");
    println!("elapsed:        {elapsed:?}");
    println!(
        "throughput:     {:.0} reserves/sec",
        THREADS as f64 / elapsed.as_secs_f64()
    );

    // Invariants. The binary uses assert! deliberately: this is a smoke
    // tool, not library code, and a panic is the right failure signal.
    assert_eq!(
        accepted as u32, STOCK,
        "exactly STOCK reservations must be accepted"
    );
    assert_eq!(
        accepted + out_of_stock,
        THREADS,
        "every thread must terminate with Ok or OutOfStock"
    );
    assert_eq!(available, 0, "no stock should remain after the storm");
}
