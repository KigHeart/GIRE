use gire_domain::{AssetId, EvidenceId, FeederId, MeasurementQuality, Phase, SourceId, TerminalId};
use gire_electrical::{
    ConductorElectricalData, ElectricalParameterSet, LineElectricalParameters,
    SeriesImpedancePerKm, ShuntAdmittancePerKm,
};
use gire_equipment::{EquipmentModel, EquipmentTerminal, Substation, VoltageLevel};
use gire_operating_point::{LoadPhase, OperatingPoint, SourcePhase};
use gire_powerflow::{SolverOptions, prepare_radial_feeder};
use gire_topology::{BranchKind, FeederTopology, NetworkBranch, NetworkTerminal};

#[test]
fn validated_network_is_converted_and_provenance_is_retained() {
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
            name: "Adapter test".into(),
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
            kind: BranchKind::Line,
            switch_position: None,
        }],
    };

    let parameters = ElectricalParameterSet {
        revision: 4,
        equipment_revision: 7,
        lines: vec![LineElectricalParameters {
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
        }],
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

    let prepared = prepare_radial_feeder(
        &topology,
        &equipment,
        &parameters,
        &operating_point,
        Phase::A,
    )
    .unwrap();
    assert_eq!(prepared.provenance.topology_revision, 3);
    assert_eq!(prepared.provenance.equipment_revision, 7);
    assert_eq!(prepared.provenance.electrical_parameter_revision, 4);
    assert_eq!(prepared.provenance.operating_point_revision, 1);

    let solved = prepared.solve(SolverOptions::default()).unwrap();
    assert!(solved.result.terminal(load_terminal).is_some());
    assert!(!solved.has_ampacity_violations());
}
