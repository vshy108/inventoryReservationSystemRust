use super::ReservationError;

/// Domain-level trace events emitted by [`crate::ReservationService`] on every
/// transition and rejection path.
///
/// Collected through the [`TraceSubscriber`] port so the core never depends on
/// logging or I/O crates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceEvent {
    /// `reserve_item` succeeded — a unit is now held.
    Reserved {
        reservation_id: String,
        product_id: String,
        user_id: String,
    },
    /// `reserve_item` was rejected (e.g. `OutOfStock`, `ProductNotFound`).
    ReserveFailed {
        product_id: String,
        user_id: String,
        error: ReservationError,
    },
    /// `confirm_reservation` succeeded.
    Confirmed { reservation_id: String },
    /// `confirm_reservation` was rejected.
    ConfirmFailed {
        reservation_id: String,
        error: ReservationError,
    },
    /// `cancel_reservation` succeeded.
    Cancelled { reservation_id: String },
    /// `cancel_reservation` was rejected.
    CancelFailed {
        reservation_id: String,
        error: ReservationError,
    },
    /// A reservation was swept to `Expired` by `expire_reservations`.
    Expired { reservation_id: String },
}

/// Port for receiving trace events from [`crate::ReservationService`].
///
/// Implement this trait to route events to a logger, metrics pipeline, or a
/// test recorder without coupling the core to any I/O crate.
///
/// # Example (test recorder)
/// ```
/// use std::sync::{Arc, Mutex};
/// use inventory_reservation::{TraceEvent, TraceSubscriber};
///
/// #[derive(Debug)]
/// struct Recorder(Mutex<Vec<TraceEvent>>);
///
/// impl TraceSubscriber for Recorder {
///     fn record(&self, event: TraceEvent) {
///         self.0.lock().unwrap().push(event);
///     }
/// }
/// ```
pub trait TraceSubscriber: Send + Sync {
    fn record(&self, event: TraceEvent);
}

/// No-op subscriber used as the default. All events are silently discarded.
#[derive(Debug)]
pub(crate) struct NoopSubscriber;

impl TraceSubscriber for NoopSubscriber {
    fn record(&self, _event: TraceEvent) {}
}
