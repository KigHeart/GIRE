# ADR-0005: Version-pinned operating-point model

- Status: Accepted
- Date: 2026-11-19

## Context

A load-flow engine must not infer source voltage, demand, or transformer tap
positions from topology or equipment ratings. Those are operational inputs,
distinct from the static equipment and electrical-parameter models.

## Decision

Introduce `gire-operating-point` with explicit per-phase source reference
voltages, per-phase constant-P/Q load values, and observed transformer tap
states. Source voltages are RMS line-to-neutral volts with angles in degrees.
Positive load P/Q denotes consumption; negative values are explicit export.
Every observation carries quality, timestamp, and evidence identity.

An operating point pins topology, equipment, and electrical-parameter
revisions. Validation checks equipment revision, terminal/phase references,
numeric domains, transformer identity, tap range, and observation quality.
Bad, stale, or missing observations are rejected. Suspect and estimated
observations remain represented but make the point ineligible for deterministic
solver execution.

## Consequences

This crate is a validated input contract, not a solver. The first solver must
also verify topology and electrical-parameter revision matches, source
connectivity, source-reference uniqueness/compatibility, and model coverage.
Tap states are explicit observations; this ADR does not assert that a
transformer has a physical on-load/off-circuit tap changer unless the
equipment model is extended to represent that capability.
