# GIRE — Grid Intelligence and Resilience Engine

GIRE is an advisory-first, Rust-based engineering system for electrical
distribution operations, developed by Tilil Engineering Group.

The initial development profile is read-only. AI agents do not have
independent authority to operate electrical equipment. Protection systems,
interlocks, permits, and utility switching procedures remain outside agent
authority.

## Engineering status

The repository currently contains:

- `gire-domain`: typed electrical identifiers, phases, measurement quality,
  engineering units, and explicit unknown switch-state semantics.
- `gire-evidence`: canonical operational event envelopes with SHA-256
  integrity checks.
- `gire-topology`: deterministic, phase-aware feeder connectivity
  reconstruction for phases A, B, C, and neutral.
- `gire-equipment`: validated substation, voltage-level, busbar, terminal,
  and two-winding transformer models with explicit winding connections,
  rated voltages, vector-group clock and phase correspondence.

## Development sequence

1. Typed domain model and evidence provenance.
2. Canonical event ingestion and deterministic replay.
3. Versioned feeder topology and connectivity validation.
4. Explicit electrical equipment model and transformer semantics.
5. Deterministic electrical analysis (after required parameters and source
   conditions are modelled and validated).
6. Bounded agents and a policy/authority kernel.

## Local development

Install the stable Rust toolchain, then run:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

GIRE is not connected to any utility operational network. No live SCADA,
OMS, GIS, AMI, or switching integration is implied by this repository.
