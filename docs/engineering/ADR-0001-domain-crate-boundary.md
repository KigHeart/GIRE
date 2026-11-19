# ADR-0001: Establish a dependency-light electrical domain crate

- **Status:** Accepted
- **Date:** 2026-11-19
- **Owner:** Tilil Engineering Group

## Context

GIRE needs a shared electrical vocabulary before ingestion adapters, persistence,
electrical solvers, agents, or user interfaces are introduced. If those layers
define their own representations, switch-state semantics and evidence handling
will diverge.

## Decision

Create gire-domain as the foundational Rust library. It owns stable identifier
newtypes, phase and unit enums, measurement quality, switch-position semantics,
and invariant validation.

The crate has no runtime, network, database, AI-model, or OT protocol dependency.
Its only production dependencies are serialization and UUID support.

A switch position is explicitly three-valued: open, closed, or unknown with a
reason. Unknown is not a default and must never be coerced to open or closed.
Known switch positions require a usable quality classification, observation
time, and evidence reference.

Measurements reject non-finite values. Percentages are range-checked. Values
remain in their declared units; implicit unit conversion is prohibited.

## Consequences

- Domain types can be reused by ingestion, topology, electrical analysis,
  evidence, and API crates without importing infrastructure.
- Invariant checks are deterministic and unit-testable.
- Adapters must preserve uncertainty rather than manufacture certainty.
- Unit normalization and network-specific engineering constraints remain
  explicit responsibilities of later modules.

## Deferred decisions

- Canonical event serialization and cryptographic digest format.
- CIM identifier mapping and network topology entities.
- Measurement conversion to SI/base values.
- Electrical operating limits and utility-specific validation profiles.
