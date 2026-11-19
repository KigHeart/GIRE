//! Canonical event envelopes and integrity verification for GIRE.
//!
//! Events preserve source identity, event time, ingestion time, quality, and
//! structured payload. The SHA-256 digest covers every field except itself.
//! This is an integrity checksum, not a digital signature or proof of source
//! authenticity.

use gire_domain::{MeasurementQuality, SourceId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use uuid::Uuid;

/// Event classes accepted by the initial ingestion boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Telemetry,
    SwitchStateChanged,
    ProtectionEvent,
    OperatorObservation,
    AssetStatusChanged,
}

/// SHA-256 digest of the canonical serialized event fields.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct EventDigest(pub [u8; 32]);

impl fmt::Display for EventDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Immutable event envelope.
///
/// Payload keys are held in a BTreeMap so their ordering is deterministic.
/// The received timestamp records ingestion time; it must not overwrite the
/// source-observed timestamp.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CanonicalEvent {
    pub event_id: Uuid,
    pub source_id: SourceId,
    pub kind: EventKind,
    pub schema_version: u16,
    pub observed_at_unix_ms: u64,
    pub received_at_unix_ms: u64,
    pub quality: MeasurementQuality,
    pub payload: BTreeMap<String, serde_json::Value>,
    pub digest: EventDigest,
}

#[derive(Serialize)]
struct UnsignedEvent<'a> {
    event_id: Uuid,
    source_id: SourceId,
    kind: EventKind,
    schema_version: u16,
    observed_at_unix_ms: u64,
    received_at_unix_ms: u64,
    quality: MeasurementQuality,
    payload: &'a BTreeMap<String, serde_json::Value>,
}

impl CanonicalEvent {
    /// Constructs an event and calculates its integrity digest.
    pub fn new(
        source_id: SourceId,
        kind: EventKind,
        schema_version: u16,
        observed_at_unix_ms: u64,
        received_at_unix_ms: u64,
        quality: MeasurementQuality,
        payload: BTreeMap<String, serde_json::Value>,
    ) -> Result<Self, EventError> {
        if schema_version == 0 {
            return Err(EventError::UnsupportedSchemaVersion);
        }

        let mut event = Self {
            event_id: Uuid::new_v4(),
            source_id,
            kind,
            schema_version,
            observed_at_unix_ms,
            received_at_unix_ms,
            quality,
            payload,
            digest: EventDigest([0; 32]),
        };
        event.digest = event.calculate_digest()?;
        Ok(event)
    }

    /// Recomputes the digest and compares it with the stored digest.
    pub fn verify_integrity(&self) -> Result<(), EventError> {
        if self.schema_version == 0 {
            return Err(EventError::UnsupportedSchemaVersion);
        }
        let calculated = self.calculate_digest()?;
        if calculated != self.digest {
            return Err(EventError::DigestMismatch);
        }
        Ok(())
    }

    fn calculate_digest(&self) -> Result<EventDigest, EventError> {
        let unsigned = UnsignedEvent {
            event_id: self.event_id,
            source_id: self.source_id,
            kind: self.kind,
            schema_version: self.schema_version,
            observed_at_unix_ms: self.observed_at_unix_ms,
            received_at_unix_ms: self.received_at_unix_ms,
            quality: self.quality,
            payload: &self.payload,
        };
        let canonical = serde_json::to_vec(&unsigned)
            .map_err(|error| EventError::Serialization(error.to_string()))?;
        Ok(EventDigest(Sha256::digest(canonical).into()))
    }
}

/// Errors raised while constructing or verifying canonical events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventError {
    UnsupportedSchemaVersion,
    Serialization(String),
    DigestMismatch,
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion => f.write_str("schema version must be non-zero"),
            Self::Serialization(message) => write!(f, "event serialization failed: {message}"),
            Self::DigestMismatch => f.write_str("event digest does not match event content"),
        }
    }
}

impl std::error::Error for EventError {}
