use gire_domain::{
    AssetId, DomainError, EngineeringUnit, EvidenceId, Measurement, MeasurementQuality, Phase,
    SwitchPosition, SwitchState, UnknownSwitchReason,
};

#[test]
fn unknown_switch_state_is_a_first_class_valid_state() {
    let state = SwitchState {
        asset_id: AssetId::new(),
        position: SwitchPosition::Unknown(UnknownSwitchReason::ConflictingEvidence),
        quality: MeasurementQuality::Suspect,
        observed_at_unix_ms: None,
        evidence_id: None,
    };

    assert_eq!(state.validate(), Ok(()));
}

#[test]
fn known_switch_state_requires_time_and_evidence() {
    let state = SwitchState {
        asset_id: AssetId::new(),
        position: SwitchPosition::Closed,
        quality: MeasurementQuality::Good,
        observed_at_unix_ms: Some(1_800_000_000_000),
        evidence_id: None,
    };

    assert_eq!(state.validate(), Err(DomainError::MissingEvidence));
}

#[test]
fn stale_telemetry_cannot_assert_a_known_switch_position() {
    let state = SwitchState {
        asset_id: AssetId::new(),
        position: SwitchPosition::Open,
        quality: MeasurementQuality::Stale,
        observed_at_unix_ms: Some(1_800_000_000_000),
        evidence_id: Some(EvidenceId::new()),
    };

    assert_eq!(
        state.validate(),
        Err(DomainError::UnusableKnownSwitchQuality)
    );
}

#[test]
fn finite_measurement_with_provenance_is_valid() {
    let measurement = Measurement {
        asset_id: AssetId::new(),
        phase: Some(Phase::B),
        value: 11_000.0,
        unit: EngineeringUnit::Volt,
        quality: MeasurementQuality::Good,
        observed_at_unix_ms: 1_800_000_000_000,
        evidence_id: EvidenceId::new(),
    };

    assert_eq!(measurement.validate(), Ok(()));
}

#[test]
fn non_finite_measurements_are_rejected() {
    let measurement = Measurement {
        asset_id: AssetId::new(),
        phase: None,
        value: f64::NAN,
        unit: EngineeringUnit::Watt,
        quality: MeasurementQuality::Good,
        observed_at_unix_ms: 1_800_000_000_000,
        evidence_id: EvidenceId::new(),
    };

    assert_eq!(
        measurement.validate(),
        Err(DomainError::NonFiniteMeasurement)
    );
}

#[test]
fn percentage_measurements_are_range_checked() {
    let measurement = Measurement {
        asset_id: AssetId::new(),
        phase: None,
        value: 101.0,
        unit: EngineeringUnit::Percent,
        quality: MeasurementQuality::Estimated,
        observed_at_unix_ms: 1_800_000_000_000,
        evidence_id: EvidenceId::new(),
    };

    assert_eq!(measurement.validate(), Err(DomainError::PercentOutOfRange));
}
