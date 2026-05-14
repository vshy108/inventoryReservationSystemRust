# ADR-001: Persistence Scope

**Date:** 2026-05-14  
**Status:** Accepted  
**Deciders:** repo maintainers  

---

## Context

`ReservationService` holds all state in process memory. If the process restarts,
all products, reservations, and the routing index are lost. The question is
whether a durable persistence adapter belongs in this repository.

---

## Decision

**A durable adapter is out of scope for this repository.**

The repo's purpose is to demonstrate correct concurrency control for in-memory
flash-sale reservations. Adding a real database adapter would shift the focus
from the concurrency problem to infrastructure plumbing, without teaching
additional domain concepts.

If persistence is needed in a production system it should live in a separate
service layer that wraps this library, not inside the core.

---

## Required Invariants for a Future Adapter

If a persistence adapter is added later (in this or a downstream repo), the
following invariants must be preserved on every reload:

| Invariant | Description |
|---|---|
| **Stock conservation** | `confirmed_count + active_reservation_count <= total_stock` for every product after reload. Active reservations that expired while the process was down must be swept before the service starts accepting new requests. |
| **Reservation identity** | Every `reservation_id` in the reservation map must also appear in `reservation_index`, and vice versa. No orphan entries. |
| **Monotonic ID counter** | `next_id` must be restored to a value strictly greater than the highest `reservation_id` seen in the loaded data, so freshly issued IDs never collide with persisted ones. |
| **Terminal state is final** | `Confirmed`, `Cancelled`, and `Expired` reservations must never transition again. Reloading a terminal reservation must not re-insert it into `active_reservation_count`. |
| **Expiry on reload** | Any `Active` reservation with `expires_at < now_at_reload` must be transitioned to `Expired` and its stock released before the service becomes available. |

---

## Tests Required Before Any Adapter Implementation

All of the following tests must be written (and failing) before any adapter
code is introduced, per the TDD discipline in `AGENTS.md`:

1. **Round-trip product**: `seed_product` → persist → reload → `get_available_stock`
   returns the original total.

2. **Round-trip active reservation**: `reserve_item` → persist → reload →
   `get_reservation` returns an `Active` reservation with the same
   `reservation_id`, `product_id`, `user_id`, `expires_at`.

3. **Active reservation confirmation after reload**: `reserve_item` → persist →
   reload → `confirm_reservation` succeeds.

4. **Expired-on-reload sweep**: `reserve_item` with `expires_at` in the past →
   persist → reload → `get_available_stock` reflects released stock (the
   reservation was swept during reload, not left as `Active`).

5. **ID monotonicity after reload**: persist N reservations → reload →
   `reserve_item` → new `reservation_id` is lexicographically and numerically
   greater than all persisted IDs.

6. **No orphan index entries after reload**: load corrupt data that has a
   `reservation_id` in the map but not in the index (or vice versa) → service
   refuses to start (returns an error) rather than silently operating with a
   broken index.

---

## Consequences

- The core library (`src/`) remains I/O-free and has no persistence dependencies.
- `infrastructure/` and `interface/` source directories remain empty scaffolding
  until a concrete adapter is chosen and its tests are written.
- Any future adapter must pass the six invariant tests above before being merged.
