//! Clock trait contract: SystemClock returns a real timestamp; a hand-rolled
//! fake can be plugged in for deterministic tests.

use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, TimeZone, Utc};
use inventory_reservation::{Clock, SystemClock};

#[test]
fn system_clock_returns_a_recent_timestamp() {
    let before = Utc::now();
    let observed = SystemClock.now();
    let after = Utc::now();
    assert!(before <= observed && observed <= after);
}

#[test]
fn fake_clock_can_be_substituted_for_system_clock() {
    struct FakeClock {
        ticks: AtomicI64,
    }

    impl Clock for FakeClock {
        fn now(&self) -> DateTime<Utc> {
            let secs = self.ticks.fetch_add(1, Ordering::Relaxed);
            Utc.timestamp_opt(secs, 0).unwrap()
        }
    }

    let clock: Box<dyn Clock> = Box::new(FakeClock {
        ticks: AtomicI64::new(1_700_000_000),
    });
    let t1 = clock.now();
    let t2 = clock.now();
    assert!(t2 > t1);
    assert_eq!((t2 - t1).num_seconds(), 1);
}
