# GIRE — Grid Intelligence and Resilience Engine

GIRE is an advisory-first, Rust-based engineering system for electrical
distribution operations, developed by Tilil Engineering Group.

The initial development profile is read-only. AI agents do not have
independent authority to operate electrical equipment. Protection systems,
interlocks, permits, and utility switching procedures remain outside agent
authority.

## Engineering status

- `gire-domain`: typed identifiers, phases, measurement quality, units and
  explicit unknown switch-state semantics.
- `gire-evidence`: canonical operational event envelopes with SHA-256
  integrity checks.
- `gire-topology`: deterministic phase-aware feeder connectivity.
- `gire-equipment`: validated substations, voltage levels, busbars,
  terminals and explicit two-winding transformer models.
- `gire-electrical`: SI-unit line/cable conductor data and per-unit
  transformer sequence parameters, validated against an equipment revision.
- `gire-operating-point`: version-pinned per-phase source voltages, constant-P/Q
  load observations, and explicit transformer tap states with evidence quality.
- `gire-network-validation`: cross-layer revision, terminal, parameter-coverage
  and source-reachability validation; unknown-switch paths remain indeterminate.
- `gire-powerflow`: first single-phase radial backward/forward-sweep numerical
  kernel with explicit series-only assumptions and convergence diagnostics.

## Development sequence

1. Typed domain model and evidence provenance.
2. Canonical event ingestion and deterministic replay.
3. Versioned feeder topology and connectivity validation.
4. Explicit electrical equipment and transformer semantics.
5. Electrical parameter validation and coverage.
6. Version-pinned operating-point inputs and observation quality.
7. Deterministic three-phase unbalanced analysis, only when required source,
   load, topology and parameter data are present.
8. Bounded agents and a policy/authority kernel.

## Local development

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

No live SCADA, OMS, GIS, AMI, or switching integration is implied.
