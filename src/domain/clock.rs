//! Clock abstraction.
//!
//! All public service methods accept a `now: DateTime<Utc>` so tests can pin
//! time deterministically. The `Clock` trait is the seam for callers that
//! want to delegate "what time is it?" to a single object — for example, a
//! future HTTP handler that injects `SystemClock` in production and a fake
//! clock in tests without threading `now` through every call site.

use chrono::{DateTime, Utc};

/// Source of "now" for callers that don't want to pass a timestamp explicitly.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// Production clock backed by `chrono::Utc::now()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
