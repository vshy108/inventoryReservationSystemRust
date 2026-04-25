use chrono::{DateTime, Utc};

/// Lifecycle state of a [`Reservation`].
///
/// Valid transitions:
///
/// * `Active -> Confirmed` (purchase finalised before expiry).
/// * `Active -> Cancelled` (user or admin released the hold).
/// * `Active -> Expired` (`now > expires_at` swept by `expire_reservations`).
///
/// `Confirmed`, `Cancelled`, and `Expired` are terminal: any further
/// transition attempt returns [`crate::ReservationError::ReservationAlreadyFinalized`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservationState {
    /// Held but not yet finalised. Counts against `available_stock`
    /// until the reservation is confirmed, cancelled, or expires.
    Active,
    /// Sale completed. Permanently counts against `total_stock`.
    Confirmed,
    /// Released before expiry. Stock returns to `available_stock`.
    Cancelled,
    /// `now > expires_at` was observed by `expire_reservations` or
    /// `confirm_reservation`. Stock returns to `available_stock`.
    Expired,
}

/// A single reservation against one unit of one product, owned by one user.
///
/// Timestamps record when each terminal transition happened (or `None`
/// while the reservation is still `Active`). The reservation's `state`
/// is the source of truth; the `*_at` fields are observability metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    /// Service-assigned unique identifier (currently a counter; opaque to callers).
    pub reservation_id: String,
    /// SKU this reservation holds a unit of.
    pub product_id: String,
    /// Caller-supplied user identifier (no validation performed by the library).
    pub user_id: String,
    /// Current lifecycle position. See [`ReservationState`].
    pub state: ReservationState,
    /// Wall-clock time at which the reservation was first created.
    pub created_at: DateTime<Utc>,
    /// `created_at + RESERVATION_HOLD`. After this point the reservation
    /// is eligible to be swept to [`ReservationState::Expired`].
    pub expires_at: DateTime<Utc>,
    /// Set when state transitioned to [`ReservationState::Confirmed`].
    pub confirmed_at: Option<DateTime<Utc>>,
    /// Set when state transitioned to [`ReservationState::Cancelled`].
    pub cancelled_at: Option<DateTime<Utc>>,
    /// Set when state transitioned to [`ReservationState::Expired`].
    pub expired_at: Option<DateTime<Utc>>,
}
