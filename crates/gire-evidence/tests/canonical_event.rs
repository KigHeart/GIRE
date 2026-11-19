use gire_domain::{MeasurementQuality, SourceId};
use gire_evidence::{CanonicalEvent, EventError, EventKind};
use std::collections::BTreeMap;

fn payload(entries: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn event(payload: BTreeMap<String, serde_json::Value>) -> CanonicalEvent {
    CanonicalEvent::new(
        SourceId::new(),
        EventKind::Telemetry,
        1,
        1_800_000_000_000,
        1_800_000_000_125,
        MeasurementQuality::Good,
        payload,
    )
    .expect("valid event")
}

#[test]
fn event_digest_verifies_after_construction() {
    let event = event(payload(&[("voltage_v", serde_json::json!(11000.0))]));
    assert_eq!(event.verify_integrity(), Ok(()));
}

#[test]
fn payload_mutation_is_detected() {
    let mut event = event(payload(&[("voltage_v", serde_json::json!(11000.0))]));
    event
        .payload
        .insert("voltage_v".to_owned(), serde_json::json!(10000.0));

    assert_eq!(event.verify_integrity(), Err(EventError::DigestMismatch));
}

#[test]
fn schema_version_zero_is_rejected() {
    let result = CanonicalEvent::new(
        SourceId::new(),
        EventKind::Telemetry,
        0,
        1_800_000_000_000,
        1_800_000_000_125,
        MeasurementQuality::Good,
        BTreeMap::new(),
    );

    assert_eq!(result, Err(EventError::UnsupportedSchemaVersion));
}

#[test]
fn serialized_event_round_trips_and_retains_digest() {
    let original = event(payload(&[
        ("current_a", serde_json::json!(14.2)),
        ("voltage_v", serde_json::json!(11000.0)),
    ]));
    let bytes = serde_json::to_vec(&original).expect("serialize");
    let decoded: CanonicalEvent = serde_json::from_slice(&bytes).expect("deserialize");

    assert_eq!(decoded, original);
    assert_eq!(decoded.verify_integrity(), Ok(()));
}

#[test]
fn payload_insertion_order_produces_identical_canonical_payload_bytes() {
    let first = event(payload(&[
        ("a", serde_json::json!(1)),
        ("b", serde_json::json!(2)),
    ]));
    let second = event(payload(&[
        ("b", serde_json::json!(2)),
        ("a", serde_json::json!(1)),
    ]));

    // Event identity is intentionally distinct, so compare canonical payload
    // bytes rather than whole-event digests.
    let first_payload = serde_json::to_vec(&first.payload).expect("serialize payload");
    let second_payload = serde_json::to_vec(&second.payload).expect("serialize payload");
    assert_eq!(first_payload, second_payload);
}
