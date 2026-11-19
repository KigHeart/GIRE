# ADR-0004: Electrical parameter model

- Status: Accepted
- Date: 2026-11-19

## Context

Topology and equipment identity alone are insufficient for electrical
calculations. A load-flow engine must not invent line impedance, shunt
admittance, conductor ratings, transformer sequence impedances or base values.

## Decision

Add `gire-electrical`, separate from the graph and equipment identity crates.
Line/cable conductor records carry per-kilometre series impedance and shunt
admittance in SI-derived units, physical route length in metres, and continuous
ampacity in amperes. Transformer records carry rated apparent power and
positive-, negative- and optional zero-sequence impedance in per unit, plus
magnetising admittance on the same declared base.

Every parameter set is pinned to an equipment-model revision. Validation
checks references, phase coverage at both line terminals, finite values,
non-negative dissipative terms, positive ratings, and transformer references.
An absent zero-sequence value remains unknown; it is never replaced by zero.

## Consequences

- Electrical data is version-bound and independently auditable.
- Values have explicit units and bases.
- This is a validated parameter layer, not a power-flow implementation.
- Temperature correction, mutual coupling, frequency dependence, detailed
  earth-return models, transformer tap/changer models, and CIM import remain
  future work and must be explicit before calculations depend on them.
