#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReservationError {
    #[error("out of stock")]
    OutOfStock,
    #[error("reservation not found")]
    ReservationNotFound,
    #[error("reservation already finalized")]
    ReservationAlreadyFinalized,
    #[error("reservation expired")]
    ReservationExpired,
    #[error("product not found")]
    ProductNotFound,
}
