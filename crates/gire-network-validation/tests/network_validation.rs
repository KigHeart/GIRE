use gire_domain::{
    AssetId, EvidenceId, FeederId, MeasurementQuality, Phase, SourceId, SwitchPosition,
    TerminalId, UnknownSwitchReason,
};
use gire_electrical::{
    ConductorElectricalData, ElectricalParameterSet, LineElectricalParameters,
    SeriesImpedancePerKm, ShuntAdmittancePerKm,
};
use gire_equipment::{EquipmentModel, EquipmentTerminal, Substation, VoltageLevel};
use gire_operating_point::{LoadPhase, OperatingPoint, SourcePhase};
use gire_topology::{BranchKind, FeederTopology, NetworkBranch, NetworkTerminal};
use gire_network_validation::{
    validate_network, LoadSupplyStatus, NetworkValidationError,
};

fn fixture(switch_position: Option<SwitchPosition>) -> (
    FeederTopology,
    EquipmentModel,
    ElectricalParameterSet,
    OperatingPoint,
) {
    let substation_id = AssetId::new();
    let voltage_id = AssetId::new();
    let line_id = AssetId::new();
    let source_terminal = TerminalId::new();
    let load_terminal = TerminalId::new();
    let phases = vec![Phase::A];

    let equipment = EquipmentModel {
        revision: 7,
        substations: vec![Substation {
            id: substation_id,
            name: "Validation test substation".into(),
        }],
        voltage_levels: vec![VoltageLevel {
            id: voltage_id,
            substation_id,
            nominal_voltage_volts: 11_000.0,
        }],
        terminals: vec![
            EquipmentTerminal {
                id: source_terminal,
                equipment_id: line_id,
                voltage_level_id: voltage_id,
                phases: phases.clone(),
            },
            EquipmentTerminal {
                id: load_terminal,
                equipment_id: line_id,
                voltage_level_id: voltage_id,
                phases: phases.clone(),
            },
        ],
        busbars: vec![],
        transformers: vec![],
    };

    let kind = if switch_position.is_some() {
        BranchKind::Switch
    } else {
        BranchKind::Line
    };
    let topology = FeederTopology {
        feeder_id: FeederId::new(),
        revision: 3,
        terminals: vec![
            NetworkTerminal {
                id: source_terminal,
                asset_id: line_id,
                phases: phases.clone(),
            },
            NetworkTerminal {
                id: load_terminal,
                asset_id: line_id,
                phases: phases.clone(),
            },
        ],
        branches: vec![NetworkBranch {
            asset_id: line_id,
            from_terminal: source_terminal,
            to_terminal: load_terminal,
            phases: phases.clone(),
            kind,
            switch_position,
        }],
    };

    let parameters = ElectricalParameterSet {
        revision: 4,
        equipment_revision: 7,
        lines: if switch_position.is_none() {
            vec![LineElectricalParameters {
                asset_id: line_id,
                from_terminal: source_terminal,
                to_terminal: load_terminal,
                length_metres: 100.0,
                conductors: vec![ConductorElectricalData {
                    phase: Phase::A,
                    series: SeriesImpedancePerKm {
                        resistance_ohm_per_km: 0.2,
                        reactance_ohm_per_km: 0.1,
                    },
                    shunt: ShuntAdmittancePerKm {
                        conductance_siemens_per_km: 0.0,
                        susceptance_siemens_per_km: 0.0,
                    },
                    continuous_ampacity_ampere: 100.0,
                }],
            }]
        } else {
            vec![]
        },
        transformers: vec![],
    };

    let operating_point = OperatingPoint {
        revision: 1,
        as_of_unix_ms: 1_800_000_000_000,
        topology_revision: 3,
        equipment_revision: 7,
        electrical_parameter_revision: 4,
        sources: vec![SourcePhase {
            source_id: SourceId::new(),
            terminal_id: source_terminal,
            phase: Phase::A,
            voltage_magnitude_volts_rms: 6_350.0,
            voltage_angle_degrees: 0.0,
            quality: MeasurementQuality::Good,
            observed_at_unix_ms: 1_800_000_000_000,
            evidence_id: EvidenceId::new(),
        }],
        loads: vec![LoadPhase {
            load_id: AssetId::new(),
            terminal_id: load_terminal,
            phase: Phase::A,
            active_power_watts: 10_000.0,
            reactive_power_vars: 2_000.0,
            quality: MeasurementQuality::Good,
            observed_at_unix_ms: 1_800_000_000_000,
            evidence_id: EvidenceId::new(),
        }],
        transformer_taps: vec![],
    };
    (topology, equipment, parameters, operating_point)
}

#[test]
fn connected_line_with_matching_revisions_is_confirmed_supplied() {
    let (topology, equipment, parameters, point) = fixture(None);
    let report = validate_network(&topology, &equipment, &parameters, &point).unwrap();
    assert_eq!(
        report.load_assessments[0].status,
        LoadSupplyStatus::ConfirmedSupplied
    );
    assert!(report.is_solver_eligible());
}

#[test]
fn unknown_switch_produces_indeterminate_not_confirmed_supply() {
    let (topology, equipment, parameters, point) = fixture(Some(
        SwitchPosition::Unknown(UnknownSwitchReason::ConflictingEvidence),
    ));
    let report = validate_network(&topology, &equipment, &parameters, &point).unwrap();
    assert_eq!(
        report.load_assessments[0].status,
        LoadSupplyStatus::IndeterminateAcrossUnknownSwitch
    );
    assert!(!report.is_solver_eligible());
}

#[test]
fn open_switch_produces_no_source_path() {
    let (topology, equipment, parameters, point) =
        fixture(Some(SwitchPosition::Open));
    let report = validate_network(&topology, &equipment, &parameters, &point).unwrap();
    assert_eq!(
        report.load_assessments[0].status,
        LoadSupplyStatus::NoSourcePath
    );
    assert!(!report.is_solver_eligible());
}

#[test]
fn parameter_revision_must_match_operating_point() {
    let (topology, equipment, parameters, mut point) = fixture(None);
    point.electrical_parameter_revision = 5;
    assert!(matches!(
        validate_network(&topology, &equipment, &parameters, &point),
        Err(NetworkValidationError::ElectricalParameterRevisionMismatch {
            ..
        })
    ));
}

#[test]
fn line_branch_requires_matching_parameter_record() {
    let (topology, equipment, mut parameters, point) = fixture(None);
    parameters.lines.clear();
    assert!(matches!(
        validate_network(&topology, &equipment, &parameters, &point),
        Err(NetworkValidationError::MissingLineParameters(_))
    ));
}

#[test]
fn suspect_observation_blocks_solver_eligibility_without_erasing_assessment() {
    let (topology, equipment, parameters, mut point) = fixture(None);
    point.loads[0].quality = MeasurementQuality::Suspect;
    let report = validate_network(&topology, &equipment, &parameters, &point).unwrap();
    assert_eq!(
        report.load_assessments[0].status,
        LoadSupplyStatus::ConfirmedSupplied
    );
    assert!(!report.is_solver_eligible());
}
