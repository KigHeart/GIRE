# ADR-0003: Explicit electrical equipment model

- Status: Accepted
- Date: 2026-11-19

## Context

The initial feeder topology graph represents connectivity between terminals.
A transformer cannot safely be treated as an ordinary same-phase branch:
its windings may have different voltage levels and connection types, and its
phase displacement is defined by its vector group.

## Decision

Introduce `gire-equipment` as a separate validated engineering-model layer.
It represents substations, voltage levels, equipment terminals, busbars and
two-winding transformers. Each transformer carries two explicit winding
records, rated voltages, connection types, an IEC vector-group clock number,
and a bijective phase correspondence between the represented phase sets.

The phase correspondence is model metadata; it does not assert galvanic
continuity between primary and secondary conductors. The vector-group clock
is retained independently. Grounding impedance, winding resistance/reactance,
tap changers, three-winding transformers and detailed CIM import are deferred
until they can be represented without implicit defaults.

## Consequences

- Transformer semantics are no longer conflated with line/switch graph edges.
- Referential integrity and basic electrical invariants are checked before
  downstream calculations consume the model.
- This crate does not yet perform load flow or establish operational
  energization. Unknown, missing or unsupported engineering data must remain
  explicit and block calculations that require it.
