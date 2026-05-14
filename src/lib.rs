//! Inventory Reservation System — core library.
//!
//! Spec: `docs/prompt.md` and `specs/level-*.md`.
//!
//! ## Quick example
//!
//! ```
//! use chrono::Utc;
//! use inventory_reservation::{ReservationError, ReservationService, ReservationState};
//!
//! let svc = ReservationService::new();
//! svc.seed_product("sku-1", 1);
//!
//! let reservation = svc.reserve_item("sku-1", "alice", Utc::now()).unwrap();
//! assert_eq!(reservation.state, ReservationState::Active);
//!
//! // The last unit is now held; a second reserve is rejected.
//! let err = svc.reserve_item("sku-1", "bob", Utc::now()).unwrap_err();
//! assert_eq!(err, ReservationError::OutOfStock);
//! ```

pub mod application;
pub mod domain;
pub mod interface;

pub use application::ReservationService;
pub use domain::{Clock, Reservation, ReservationError, ReservationState, SystemClock, TraceEvent, TraceSubscriber};
