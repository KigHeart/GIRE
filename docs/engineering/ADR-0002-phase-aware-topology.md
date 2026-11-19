# ADR-0002: Deterministic phase-aware feeder topology

- **Status:** Accepted
- **Date:** 2026-11-19
- **Owner:** Tilil Engineering Group

## Context

GIRE requires a deterministic representation of feeder connectivity before it
can perform load flow, fault analysis, restoration assessment, or agentic
reasoning. Connectivity must be phase-aware and must preserve uncertainty in
switch observations.

## Decision

Introduce gire-topology, depending only on gire-domain and serialization.
The input is an immutable feeder revision containing terminals and two-terminal
branches. The builder validates all references and phase compatibility before
returning any snapshot.

Connectivity is reconstructed independently for phases A, B, and C. Confirmed
closed switches and modelled non-switch branches contribute connectivity.
Confirmed open switches do not. Unknown switch positions are excluded from
confirmed connectivity and are surfaced as uncertainty.

The snapshot reports connected components, confirmed-open branches, unknown
branches, and confirmed cycle edges. Radiality is reported as:
- **Radial:** no confirmed cycle and no unknown switch on that conductor. This is a graph-acyclicity classification (a forest); without source and energization semantics it does not assert a single-source energized radial feeder.
- **Meshed:** at least one confirmed cycle exists.
- **Indeterminate:** no confirmed cycle exists, but at least one switch state
  is unknown.

Isolated terminals are retained as one-terminal components. Neutral is
reconstructed as a separate conductor graph. No partial result
is returned for an invalid model.

## Explicit scope limits

- The builder does not infer source nodes, energized status, voltage, loading,
  fault level, protection reach, or equipment availability.
- A non-switch branch is considered connected because it is present in the
  supplied model. Operational outages must be represented by a later explicit
  status dimension; they must not be encoded by deleting engineering assets.
- Transformer phase shifts, winding connections, impedance, grounding, and
  voltage-level transformations are not yet represented.
- The snapshot is advisory data, not a switching instruction or authority to
  operate equipment.

## Consequences

- Topology reconstruction is reproducible and independent of AI inference.
- Conflicting or missing switch evidence cannot silently create a closed path.
- Phase-specific graph tests can be written without relying on a solver.
- Later electrical analysis can consume a validated snapshot with explicit
  uncertainty rather than reconstructing connectivity independently.
