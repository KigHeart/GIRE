# ADR-0008: Phase-domain foundation for unbalanced distribution power flow

- Status: Proposed
- Date: 2026-11-19
- Decision owners: Tilil Engineering Group
- Scope: numerical and data-model boundary for the next GIRE power-flow capability

## Context

The current `gire-powerflow` kernel solves one single-phase radial network using a scalar complex series impedance per branch. Its adapter selects one phase and rejects unsupported branch types and non-zero shunt admittance. Running that solver three times is not a valid general three-phase solution: it omits mutual phase coupling, shared-neutral voltage drop, and phase-to-neutral load interaction.

The existing domain already represents A, B, C and Neutral connectivity independently. The electrical parameter model stores conductor series data independently, while transformer parameters include sequence values but do not yet provide a complete phase-domain transformer primitive. The operating-point model stores per-phase source voltage and per-phase constant-P/Q demand. Those inputs are not sufficient to claim a coupled four-wire solution.

## Decision

Build a separate phase-domain solver path. Preserve `solve_radial` as the stable single-phase kernel and preserve the current adapter's conservative eligibility rules. Do not silently reinterpret the current per-conductor impedance records as a coupled model.

The first coupled solver release shall support a radial, four-wire (A/B/C/N) feeder with explicit phase and neutral conductor state. It shall use a complex phase-domain primitive for each branch. The model shall make the conductor ordering explicit and immutable:

`[A, B, C, N]`

A branch primitive shall contain a 4x4 complex series impedance matrix, with a declared conductor-presence mask. Matrix entries are ohms for the complete branch length, not per-kilometre values. The matrix must be finite, reciprocal within a documented tolerance, and physically admissible under the selected validation policy. Missing entries are unknown, never zero. A branch lacking a required conductor or matrix term is ineligible.

For an explicitly three-wire branch, the neutral conductor is absent by declaration; it is not modelled as an implicit ideal ground. The initial four-wire solver shall reject mixed three-wire/four-wire transitions unless a separately specified grounding/neutral interface model is present.

## Electrical equations and sign convention

Use RMS phasors and the passive sign convention. For each branch oriented from terminal (i) to terminal (j):

`V_i - V_j = Z_{ij} I_{ij}`

where voltage and current vectors follow the declared conductor ordering. The branch current vector is positive from (i) to (j). At each non-source terminal, apply complex-current KCL across incident branches and terminal injections.

For each constant-power phase load, define positive complex power as consumption:

`S_p = P_p + j Q_p`

and phase current injection into the load as:

`I_p = conjugate(S_p / V_{pN})`

with `V_{pN} = V_p - V_N`. A load must explicitly identify its connection (phase-to-neutral or phase-to-phase); the initial implementation shall support phase-to-neutral only. Phase-to-phase, delta, ZIP, motor, inverter, and voltage-dependent models remain out of scope until their equations and validation are separately specified.

Neutral current is not an independent demand input. It is obtained from KCL and the modelled phase currents. The neutral conductor voltage is solved as part of the network state. Do not clamp neutral voltage to zero except at an explicitly modelled grounded reference boundary.

## Solver formulation

Use a backward/forward sweep over a validated radial tree, with vector currents and matrix voltage drops. For each iteration:

1. Evaluate each supported load current from the previous phase-to-neutral voltage estimate.
2. Accumulate phase and neutral currents from leaves toward the slack using vector KCL.
3. Propagate conductor voltage vectors from the slack toward leaves using each branch's matrix voltage drop.
4. Compute the maximum absolute conductor-voltage change across all energized conductors.
5. Stop only when the configured absolute tolerance is met; otherwise return a non-convergence error at the iteration limit.

The implementation must establish that the graph is a tree for the selected conductor set and that every energized load is reachable from exactly one compatible slack boundary. No disconnected energized component, cycle, unknown switch state, or ambiguous source may be silently omitted.

The first release shall not use sequence-component decoupling as a substitute for phase-domain modelling. Sequence transformations may be added later as analysis tools, not as an implicit fallback.

## Source and grounding boundary

The slack input shall provide an explicit complex voltage for every conductor present at the source terminal. The source record must declare its grounding/reference arrangement. The first implementation shall support one explicitly grounded four-wire source with a defined neutral reference. It shall reject floating-neutral, impedance-grounded, multi-grounded-neutral, and multiple-source systems until those boundary conditions are represented and validated.

The solver must not infer a grounding point from a terminal name, equipment type, or missing neutral voltage.

## Results and diagnostics

A converged result shall report:

- RMS complex voltage for each terminal and conductor;
- phase-to-neutral voltage for each supported phase load;
- branch current for every present conductor;
- branch conductor loading against the corresponding continuous ampacity;
- per-conductor complex power at both branch ends;
- per-branch real losses computed from the full matrix model;
- convergence iteration count and final maximum voltage delta;
- immutable input revision provenance.

A result shall distinguish a calculated zero from an unavailable or unmodelled quantity. Ampacity exceedance is an advisory diagnostic only. It must never trigger a switching operation or alter the network model.

## Validation and failure semantics

Reject, with typed errors:

- non-finite matrix entries or source/load values;
- missing required self or mutual impedance terms;
- inconsistent conductor masks at branch terminals;
- non-reciprocal matrices beyond the declared tolerance;
- unsupported grounding or load connection;
- absent/ambiguous slack;
- non-radial, disconnected, or indeterminate topology;
- invalid or missing conductor ampacity;
- stale or mismatched topology, equipment, electrical-parameter, and operating-point revisions;
- voltage collapse and non-convergence.

No default zero mutual impedance, ideal neutral, inferred grounding, or synthetic missing measurement is permitted.

## Compatibility and migration

The current `SeriesImpedancePerKm` and `ConductorElectricalData` remain valid for the existing uncoupled single-phase path. Add a distinct matrix parameter type and adapter path. Do not change the meaning or serialization of existing fields in-place.

The phase-domain solver shall be introduced behind a separate API, initially named `solve_unbalanced_radial`, with its own input, options, result, and error types. The existing `solve_radial` API and its test vectors must remain unchanged.

## Verification requirements

Before the solver is eligible for integration, tests must include:

1. Balanced three-phase, four-wire case whose phase voltages and currents agree with an independently calculated reference within declared tolerances.
2. Unequal single-phase loads on A, B, and C, producing non-zero neutral current where expected.
3. Non-zero mutual impedances demonstrating phase coupling (a change on one phase changes at least one other phase voltage).
4. Non-zero neutral impedance demonstrating neutral displacement.
5. Conductor-specific ampacity exceedance, including neutral overload.
6. Rejection of missing mutual terms, absent neutral data, unsupported grounding, and mismatched revisions.
7. Input-order invariance and deterministic result ordering.
8. Non-convergence and voltage-collapse failure tests.
9. Regression proving the existing single-phase solver outputs are unchanged.

Reference vectors must be independently derived (analytical or a separately implemented reference calculation); tests must not generate expected values by calling the production solver.

## Explicit non-goals

This ADR does not authorize live SCADA/OMS/AMI integration, equipment control, protection decisions, fault studies, switching recommendations, transformer tap optimization, inverter controls, dynamic simulation, or claims of IEC/IEEE model compliance. The solver remains advisory and offline until separately reviewed and authorized.
