/// Typed errors returned by [`crate::ReservationService`].
///
/// Mapped to HTTP/gRPC error codes by interface adapters; library
/// callers should match exhaustively to react to each case.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ReservationError {
    /// `available_stock == 0` for the requested product at the time of
    /// the call. Returned by `reserve_item` and never by lifecycle ops.
    #[error("out of stock")]
    OutOfStock,
    /// The supplied `reservation_id` is not known to the service. Can
    /// happen if the id is fabricated, was issued before a `seed_product`
    /// reset that cleared the index, or has been removed.
    #[error("reservation not found")]
    ReservationNotFound,
    /// The reservation has already reached a terminal state
    /// ([`crate::ReservationState::Confirmed`], [`crate::ReservationState::Cancelled`],
    /// or [`crate::ReservationState::Expired`]) and cannot transition again.
    #[error("reservation already finalized")]
    ReservationAlreadyFinalized,
    /// `confirm_reservation` was called with `now > expires_at`. The
    /// reservation is also transitioned to [`crate::ReservationState::Expired`]
    /// as a side effect so retries cannot ride a stale `Active` state.
    #[error("reservation expired")]
    ReservationExpired,
    /// The product id has not been seeded via `seed_product`.
    #[error("product not found")]
    ProductNotFound,
}
