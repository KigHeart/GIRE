# ADR-0007: First radial power-flow kernel

- Status: Accepted
- Date: 2026-11-19

## Context

GIRE has validated domain, topology, equipment, electrical-parameter and
operating-point models, but no numerical electrical solver. The first solver
must have explicit physical assumptions and independently testable behaviour.

## Decision

Add `gire-powerflow`, a deterministic backward/forward-sweep kernel for a
single-phase radial tree with one ideal slack node, constant-P/Q loads and
series branch impedances in ohms. It uses RMS line-to-neutral voltage
phasors, RMS currents, watts, vars and SI impedance. Positive load P and Q
denote consumption. Branch current results follow each branch's declared
endpoint orientation.

The solver rejects malformed, disconnected, meshed, duplicate-edge and
invalid-parameter networks. It reports convergence iterations, voltage
phasors, branch current phasors, terminal apparent powers and resistive losses.

## Explicit limitations

This first kernel is not yet an adapter over GIRE's topology, equipment,
parameter and operating-point models. It does not itself establish switch
state, revision consistency, measurement quality or source eligibility.
Those checks remain mandatory caller-side responsibilities through the
existing validation layer.

The kernel omits line shunt admittance, mutual coupling, neutral return,
transformers, voltage regulators, thermal derating and fault calculations.
It is not a three-phase or unbalanced solver and is not approved for
operational or protection decisions. A caller must not submit a network
whose required physics depend on omitted effects.

## Verification

Tests cover no-load voltage preservation, one-branch constant-P load
consistency, a two-branch radial feeder, and rejection of meshed,
disconnected and zero-impedance inputs. Adapter-level tests and independent
benchmark cases remain required before broader use.
