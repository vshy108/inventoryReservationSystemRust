//! HTTP boundary adapter — S2 of PLAN.md.
//!
//! A plain Rust struct that wraps [`ReservationService`] and translates HTTP
//! request/response concepts to and from the domain. Contains no business
//! logic: validation happens here; all decisions happen in the core.
//!
//! No HTTP framework dependency is introduced. A thin routing shim (axum,
//! actix-web, etc.) can call these methods and map the returned status codes
//! and body structs to framework response types.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::{Reservation, ReservationError, ReservationService, ReservationState};

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

/// Request body for `POST /products/{product_id}/reservations`.
pub struct ReserveRequest {
    pub user_id: String,
}

/// Response body carrying a reservation snapshot.
#[derive(Debug, PartialEq, Eq)]
pub struct ReservationBody {
    pub reservation_id: String,
    pub product_id: String,
    pub user_id: String,
    pub state: ReservationState,
}

/// Response body for `GET /products/{product_id}/stock`.
#[derive(Debug)]
pub struct StockBody {
    pub available: u32,
}

/// Response body for `POST /reservations/expire`.
#[derive(Debug)]
pub struct ExpireBody {
    pub expired_count: usize,
}

// ---------------------------------------------------------------------------
// Response / error wrappers
// ---------------------------------------------------------------------------

/// A successful adapter response: HTTP status code + typed body.
#[derive(Debug)]
pub struct AdapterResponse<T> {
    pub status: u16,
    pub body: T,
}

/// An adapter-level error: HTTP status code + human-readable message.
#[derive(Debug)]
pub struct HttpError {
    pub status: u16,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Thin HTTP boundary wrapping [`ReservationService`].
///
/// Methods correspond 1-to-1 with the endpoints in `specs/http-adapter.md`.
pub struct HttpAdapter {
    svc: Arc<ReservationService>,
}

impl HttpAdapter {
    pub fn new(svc: Arc<ReservationService>) -> Self {
        Self { svc }
    }

    // ------------------------------------------------------------------
    // POST /products/{product_id}/reservations   → 201 Created
    // ------------------------------------------------------------------

    pub fn reserve(
        &self,
        product_id: &str,
        req: ReserveRequest,
        now: DateTime<Utc>,
    ) -> Result<AdapterResponse<ReservationBody>, HttpError> {
        validate_not_empty(product_id, "product_id")?;
        validate_not_empty(&req.user_id, "user_id")?;

        self.svc
            .reserve_item(product_id, &req.user_id, now)
            .map(|r| AdapterResponse { status: 201, body: reservation_body(r) })
            .map_err(map_error)
    }

    // ------------------------------------------------------------------
    // POST /reservations/{reservation_id}/confirm   → 200 OK
    // ------------------------------------------------------------------

    pub fn confirm(
        &self,
        reservation_id: &str,
        now: DateTime<Utc>,
    ) -> Result<AdapterResponse<ReservationBody>, HttpError> {
        validate_not_empty(reservation_id, "reservation_id")?;

        self.svc
            .confirm_reservation(reservation_id, now)
            .map(|r| AdapterResponse { status: 200, body: reservation_body(r) })
            .map_err(map_error)
    }

    // ------------------------------------------------------------------
    // POST /reservations/{reservation_id}/cancel   → 200 OK
    // ------------------------------------------------------------------

    pub fn cancel(
        &self,
        reservation_id: &str,
        now: DateTime<Utc>,
    ) -> Result<AdapterResponse<ReservationBody>, HttpError> {
        validate_not_empty(reservation_id, "reservation_id")?;

        self.svc
            .cancel_reservation(reservation_id, now)
            .map(|r| AdapterResponse { status: 200, body: reservation_body(r) })
            .map_err(map_error)
    }

    // ------------------------------------------------------------------
    // GET /products/{product_id}/stock   → 200 OK
    // ------------------------------------------------------------------

    pub fn get_stock(
        &self,
        product_id: &str,
    ) -> Result<AdapterResponse<StockBody>, HttpError> {
        validate_not_empty(product_id, "product_id")?;

        self.svc
            .get_available_stock(product_id)
            .map(|available| AdapterResponse { status: 200, body: StockBody { available } })
            .map_err(map_error)
    }

    // ------------------------------------------------------------------
    // GET /reservations/{reservation_id}   → 200 OK / 404
    // ------------------------------------------------------------------

    pub fn get_reservation(
        &self,
        reservation_id: &str,
    ) -> Result<AdapterResponse<ReservationBody>, HttpError> {
        validate_not_empty(reservation_id, "reservation_id")?;

        self.svc
            .get_reservation(reservation_id)
            .map(|r| AdapterResponse { status: 200, body: reservation_body(r) })
            .ok_or_else(|| map_error(ReservationError::ReservationNotFound))
    }

    // ------------------------------------------------------------------
    // POST /reservations/expire   → 200 OK
    // ------------------------------------------------------------------

    pub fn expire(
        &self,
        now: DateTime<Utc>,
    ) -> Result<AdapterResponse<ExpireBody>, HttpError> {
        // expire_reservations only fails if a product goes missing mid-sweep,
        // which cannot happen in the current in-memory design. Map for completeness.
        self.svc
            .expire_reservations(now)
            .map(|expired_count| AdapterResponse {
                status: 200,
                body: ExpireBody { expired_count },
            })
            .map_err(map_error)
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn validate_not_empty(value: &str, field: &str) -> Result<(), HttpError> {
    if value.trim().is_empty() {
        return Err(HttpError {
            status: 400,
            message: format!("{field} must not be empty"),
        });
    }
    Ok(())
}

fn map_error(e: ReservationError) -> HttpError {
    let (status, message) = match e {
        ReservationError::ProductNotFound => (404, e.to_string()),
        ReservationError::ReservationNotFound => (404, e.to_string()),
        ReservationError::OutOfStock
        | ReservationError::ReservationAlreadyFinalized
        | ReservationError::ReservationExpired => (409, e.to_string()),
    };
    HttpError { status, message }
}

fn reservation_body(r: Reservation) -> ReservationBody {
    ReservationBody {
        reservation_id: r.reservation_id,
        product_id: r.product_id,
        user_id: r.user_id,
        state: r.state,
    }
}
