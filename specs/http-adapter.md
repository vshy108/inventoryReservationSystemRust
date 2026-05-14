# Spec — HTTP Adapter

Drives: [tests/http_adapter.rs](../tests/http_adapter.rs)

Implementation: [src/interface/http.rs](../src/interface/http.rs)

## Goal

Add a thin, synchronous HTTP boundary layer that preserves the existing sync
core and validates all user-supplied input at the edge before forwarding to
`ReservationService`. The adapter must never contain business logic — it only
translates HTTP concepts (request body, path parameters, status codes) to and
from the domain.

---

## Endpoints

| Method | Path | Core call | Success status |
|---|---|---|---|
| `POST` | `/products/{product_id}/reservations` | `reserve_item` | 201 Created |
| `POST` | `/reservations/{reservation_id}/confirm` | `confirm_reservation` | 200 OK |
| `POST` | `/reservations/{reservation_id}/cancel` | `cancel_reservation` | 200 OK |
| `GET` | `/products/{product_id}/stock` | `get_available_stock` | 200 OK |
| `GET` | `/reservations/{reservation_id}` | `get_reservation` | 200 OK |
| `POST` | `/reservations/expire` | `expire_reservations` | 200 OK |

---

## DTOs

### `POST /products/{product_id}/reservations`

Request body:
```json
{ "user_id": "alice" }
```

Success response (201):
```json
{
  "reservation_id": "rsv-0",
  "product_id": "sku-1",
  "user_id": "alice",
  "state": "Active",
  "expires_at": "2026-05-14T12:02:00Z"
}
```

### `GET /products/{product_id}/stock`

Success response (200):
```json
{ "available": 3 }
```

### `GET /reservations/{reservation_id}`

Success response (200): same schema as the reservation response above plus
`confirmed_at`, `cancelled_at`, `expired_at` (nullable ISO-8601 strings).

### `POST /reservations/expire`

Success response (200):
```json
{ "expired_count": 2 }
```

---

## Error Mapping

| `ReservationError` variant | HTTP status | message |
|---|---|---|
| `ProductNotFound` | 404 | `"product not found"` |
| `OutOfStock` | 409 | `"out of stock"` |
| `ReservationNotFound` | 404 | `"reservation not found"` |
| `ReservationAlreadyFinalized` | 409 | `"reservation already finalized"` |
| `ReservationExpired` | 409 | `"reservation expired"` |
| Input validation failure | 400 | e.g. `"user_id must not be empty"` |

---

## Input Validation (edge, not domain)

The adapter rejects before calling the core service when:

- `user_id` is empty or whitespace → 400
- path parameter `product_id` is empty → 400
- path parameter `reservation_id` is empty → 400

All other validation (stock available, state-machine transitions) is delegated
to the core.

---

## Design Constraints

- The adapter is a plain Rust struct with no async or HTTP-framework dependency.
  HTTP frameworks (axum, actix-web) wire up to it with a thin routing shim.
- `now: DateTime<Utc>` is passed in by the caller (framework shim or test) so
  the adapter remains deterministically testable without mocking a clock.
- The sync core is never touched by this slice; only `src/interface/` is added.

---

## Out of Scope for This Spec

- Async execution model (Tokio integration).
- Authentication / authorisation middleware.
- Request-body deserialization (serde / JSON parsing). DTOs are plain structs;
  serialization format is not specified here and will be added with the framework shim.
- `seed_product` endpoint (admin-only operation, separate spec).
