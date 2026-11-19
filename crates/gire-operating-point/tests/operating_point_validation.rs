use gire_domain::{AssetId, EvidenceId, MeasurementQuality, Phase, SourceId, TerminalId};
use gire_equipment::{EquipmentModel, EquipmentTerminal, Substation, VoltageLevel};
use gire_operating_point::{LoadPhase, OperatingPoint, OperatingPointError, SourcePhase};

fn fixture() -> (EquipmentModel, TerminalId) {
    let substation_id = AssetId::new();
    let voltage_id = AssetId::new();
    let terminal_id = TerminalId::new();
    (
        EquipmentModel {
            revision: 7,
            substations: vec![Substation {
                id: substation_id,
                name: "Test substation".into(),
            }],
            voltage_levels: vec![VoltageLevel {
                id: voltage_id,
                substation_id,
                nominal_voltage_volts: 11_000.0,
            }],
            terminals: vec![EquipmentTerminal {
                id: terminal_id,
                equipment_id: AssetId::new(),
                voltage_level_id: voltage_id,
                phases: vec![Phase::A, Phase::B, Phase::C],
            }],
            busbars: vec![],
            transformers: vec![],
        },
        terminal_id,
    )
}

fn base_point(terminal_id: TerminalId) -> OperatingPoint {
    OperatingPoint {
        revision: 1,
        as_of_unix_ms: 1_800_000_000_000,
        topology_revision: 3,
        equipment_revision: 7,
        electrical_parameter_revision: 4,
        sources: vec![SourcePhase {
            source_id: SourceId::new(),
            terminal_id,
            phase: Phase::A,
            voltage_magnitude_volts_rms: 6_350.0,
            voltage_angle_degrees: 0.0,
            quality: MeasurementQuality::Good,
            observed_at_unix_ms: 1_800_000_000_000,
            evidence_id: EvidenceId::new(),
        }],
        loads: vec![LoadPhase {
            load_id: AssetId::new(),
            terminal_id,
            phase: Phase::A,
            active_power_watts: 25_000.0,
            reactive_power_vars: 5_000.0,
            quality: MeasurementQuality::Good,
            observed_at_unix_ms: 1_800_000_000_000,
            evidence_id: EvidenceId::new(),
        }],
        transformer_taps: vec![],
    }
}

#[test]
fn accepts_valid_version_pinned_operating_point() {
    let (equipment, terminal) = fixture();
    let point = base_point(terminal);
    assert!(point.validate_against(&equipment).is_ok());
    assert!(point.is_solver_eligible());
}

#[test]
fn rejects_equipment_revision_mismatch() {
    let (equipment, terminal) = fixture();
    let mut point = base_point(terminal);
    point.equipment_revision = 8;
    assert!(matches!(
        point.validate_against(&equipment),
        Err(OperatingPointError::EquipmentRevisionMismatch { .. })
    ));
}

#[test]
fn rejects_phase_not_present_at_terminal() {
    let (equipment, terminal) = fixture();
    let mut point = base_point(terminal);
    point.sources[0].phase = Phase::Neutral;
    assert!(matches!(
        point.validate_against(&equipment),
        Err(OperatingPointError::PhaseNotPresent { .. })
    ));
}

#[test]
fn rejects_non_finite_source_voltage() {
    let (equipment, terminal) = fixture();
    let mut point = base_point(terminal);
    point.sources[0].voltage_magnitude_volts_rms = f64::NAN;
    assert!(matches!(
        point.validate_against(&equipment),
        Err(OperatingPointError::InvalidSourceVoltage(_, _))
    ));
}

#[test]
fn rejects_stale_load_observation() {
    let (equipment, terminal) = fixture();
    let mut point = base_point(terminal);
    point.loads[0].quality = MeasurementQuality::Stale;
    assert!(matches!(
        point.validate_against(&equipment),
        Err(OperatingPointError::UnusableObservation(
            MeasurementQuality::Stale
        ))
    ));
}

#[test]
fn suspect_values_are_retained_but_not_solver_eligible() {
    let (equipment, terminal) = fixture();
    let mut point = base_point(terminal);
    point.loads[0].quality = MeasurementQuality::Suspect;
    assert!(point.validate_against(&equipment).is_ok());
    assert!(!point.is_solver_eligible());
}

#[test]
fn rejects_duplicate_load_phase() {
    let (equipment, terminal) = fixture();
    let mut point = base_point(terminal);
    point.loads.push(point.loads[0].clone());
    assert!(matches!(
        point.validate_against(&equipment),
        Err(OperatingPointError::DuplicateLoadPhase(_, Phase::A))
    ));
}

#[test]
fn rejects_unknown_transformer_tap_reference() {
    let (equipment, terminal) = fixture();
    let mut point = base_point(terminal);
    point
        .transformer_taps
        .push(gire_operating_point::TransformerTapState {
            transformer_id: AssetId::new(),
            winding_side: gire_operating_point::WindingSide::Primary,
            minimum_position: -5,
            neutral_position: 0,
            maximum_position: 5,
            current_position: 0,
            step_voltage_percent: 1.25,
            quality: MeasurementQuality::Good,
            observed_at_unix_ms: 1_800_000_000_000,
            evidence_id: EvidenceId::new(),
        });
    assert!(matches!(
        point.validate_against(&equipment),
        Err(OperatingPointError::UnknownTransformer(_))
    ));
}
