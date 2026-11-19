# ADR-0006: Cross-layer network validation

- Status: Accepted
- Date: 2026-11-19

## Decision

Add a validation boundary that reconciles topology, equipment terminals,
electrical parameters and operating-point observations before a numerical
solver can run. Revisions must match, terminal identities and phase sets must
align, and every modelled line/transformer branch must have matching parameter
records. Line parameter endpoints are compared without assuming orientation.

For each phase-specific load, confirmed connectivity must contain exactly one
source reference to establish confirmed supply. A path that exists only when
unknown switches are treated as potentially closed is reported as
indeterminate, never as confirmed supply. An island with no source path even
under that possibility is reported as source-free.

This is a validation and classification layer, not a load-flow engine.
