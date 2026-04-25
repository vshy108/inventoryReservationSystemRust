//! Inventory Reservation System — core library.
//!
//! Spec: `docs/prompt.md` and `specs/level-*.md`.

pub mod application;
pub mod domain;

pub use application::ReservationService;
pub use domain::{Reservation, ReservationError, ReservationState};
