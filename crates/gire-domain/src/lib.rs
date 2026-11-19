//! Foundational, serializable domain types for GIRE.
//!
//! This crate deliberately contains no network clients, database access, model
//! calls, agent runtime, or control-system integration. It defines the typed
//! vocabulary used by those layers.
//!
//! Safety invariant: an unobserved, stale, conflicting, or invalid switch
//! position is represented explicitly as unknown. It is never coerced to
//! open or closed.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Stable identity newtypes prevent accidental interchange of unrelated IDs.
macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates a new random identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an existing UUID.
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            /// Returns the underlying UUID.
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(AssetId);
id_type!(FeederId);
id_type!(TerminalId);
id_type!(EvidenceId);
id_type!(SourceId);

/// Electrical phase designation. Neutral is explicit and is not a phase.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    A,
    B,
    C,
    Neutral,
}

/// Quality assigned to an observation by ingestion and validation layers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementQuality {
    Good,
    Suspect,
    Bad,
    Stale,
    Missing,
    Estimated,
}

/// Why a switch position cannot safely be asserted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownSwitchReason {
    NotObserved,
    ConflictingEvidence,
    StaleTelemetry,
    InvalidTelemetry,
    CommunicationLost,
    ModelIncomplete,
}

/// A switch position is a three-valued state, not a boolean.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "reason")]
pub enum SwitchPosition {
    Open,
    Closed,
    Unknown(UnknownSwitchReason),
}

/// A switch observation tied to its source evidence and observation time.
///
/// observed_at_unix_ms is UTC Unix time in milliseconds. It is optional for
/// unknown states because the absence of an observation is itself meaningful.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SwitchState {
    pub asset_id: AssetId,
    pub position: SwitchPosition,
    pub quality: MeasurementQuality,
    pub observed_at_unix_ms: Option<u64>,
    pub evidence_id: Option<EvidenceId>,
}

impl SwitchState {
    /// Validates that a known position is supported by usable, time-stamped
    /// evidence. Unknown states remain valid and are never defaulted.
    pub fn validate(&self) -> Result<(), DomainError> {
        match self.position {
            SwitchPosition::Unknown(_) => Ok(()),
            SwitchPosition::Open | SwitchPosition::Closed => {
                if self.quality == MeasurementQuality::Bad
                    || self.quality == MeasurementQuality::Stale
                    || self.quality == MeasurementQuality::Missing
                {
                    return Err(DomainError::UnusableKnownSwitchQuality);
                }
                if self.observed_at_unix_ms.is_none() {
                    return Err(DomainError::MissingObservationTime);
                }
                if self.evidence_id.is_none() {
                    return Err(DomainError::MissingEvidence);
                }
                Ok(())
            }
        }
    }
}

/// Canonical engineering units supported by the initial domain model.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineeringUnit {
    Volt,
    Ampere,
    Watt,
    Var,
    Hertz,
    Celsius,
    Percent,
}

/// A scalar electrical or environmental measurement.
///
/// The value is retained as supplied in the declared unit. Conversion and
/// base-value normalization belong in a later, explicit engineering layer.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Measurement {
    pub asset_id: AssetId,
    pub phase: Option<Phase>,
    pub value: f64,
    pub unit: EngineeringUnit,
    pub quality: MeasurementQuality,
    pub observed_at_unix_ms: u64,
    pub evidence_id: EvidenceId,
}

impl Measurement {
    /// Validates numeric and provenance invariants without guessing missing
    /// values or silently changing units.
    pub fn validate(&self) -> Result<(), DomainError> {
        if !self.value.is_finite() {
            return Err(DomainError::NonFiniteMeasurement);
        }
        if self.unit == EngineeringUnit::Percent && !(0.0..=100.0).contains(&self.value) {
            return Err(DomainError::PercentOutOfRange);
        }
        Ok(())
    }
}

/// Errors raised by domain-level invariant checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomainError {
    MissingObservationTime,
    MissingEvidence,
    UnusableKnownSwitchQuality,
    NonFiniteMeasurement,
    PercentOutOfRange,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingObservationTime => "known switch position requires an observation time",
            Self::MissingEvidence => "known switch position requires an evidence reference",
            Self::UnusableKnownSwitchQuality => {
                "known switch position cannot have bad, stale, or missing quality"
            }
            Self::NonFiniteMeasurement => "measurement value must be finite",
            Self::PercentOutOfRange => "percentage measurement must be between 0 and 100",
        };
        f.write_str(message)
    }
}

impl std::error::Error for DomainError {}
