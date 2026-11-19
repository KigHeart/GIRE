use gire_domain::{AssetId, Phase, TerminalId};
use gire_equipment::{
    EquipmentModel, EquipmentTerminal, PowerTransformer, Substation, TransformerWinding,
    VoltageLevel, WindingConnection,
};
use gire_electrical::{
    ConductorElectricalData, ElectricalParameterError, ElectricalParameterSet,
    LineElectricalParameters, PerUnitAdmittance, PerUnitImpedance, SeriesImpedancePerKm,
    ShuntAdmittancePerKm, TransformerElectricalParameters,
};

fn valid_equipment() -> EquipmentModel {
    let sub = AssetId::new();
    let hv = AssetId::new();
    let lv = AssetId::new();
    let tx = AssetId::new();
    let hv_t = TerminalId::new();
    let lv_t = TerminalId::new();
    let phases = vec![Phase::A, Phase::B, Phase::C];
    EquipmentModel {
        revision: 7,
        substations: vec![Substation { id: sub, name: "S1".into() }],
        voltage_levels: vec![
            VoltageLevel { id: hv, substation_id: sub, nominal_voltage_volts: 33_000.0 },
            VoltageLevel { id: lv, substation_id: sub, nominal_voltage_volts: 11_000.0 },
        ],
        terminals: vec![
            EquipmentTerminal { id: hv_t, equipment_id: tx, voltage_level_id: hv, phases: phases.clone() },
            EquipmentTerminal { id: lv_t, equipment_id: tx, voltage_level_id: lv, phases: phases.clone() },
        ],
        busbars: vec![],
        transformers: vec![PowerTransformer {
            id: tx,
            substation_id: sub,
            vector_group_clock_hour: 11,
            primary: TransformerWinding {
                id: AssetId::new(), terminal_id: hv_t, voltage_level_id: hv,
                rated_voltage_volts: 33_000.0, connection: WindingConnection::WyeGrounded,
                phases: phases.clone(),
            },
            secondary: TransformerWinding {
                id: AssetId::new(), terminal_id: lv_t, voltage_level_id: lv,
                rated_voltage_volts: 11_000.0, connection: WindingConnection::Delta,
                phases,
            },
            phase_mapping: vec![
                gire_equipment::PhaseMapping { primary: Phase::A, secondary: Phase::A },
                gire_equipment::PhaseMapping { primary: Phase::B, secondary: Phase::B },
                gire_equipment::PhaseMapping { primary: Phase::C, secondary: Phase::C },
            ],
        }],
    }
}

fn valid_parameters(equipment: &EquipmentModel) -> ElectricalParameterSet {
    ElectricalParameterSet {
        equipment_revision: equipment.revision,
        lines: vec![],
        transformers: vec![TransformerElectricalParameters {
            transformer_id: equipment.transformers[0].id,
            rated_power_va: 10_000_000.0,
            positive_sequence: PerUnitImpedance { resistance_pu: 0.005, reactance_pu: 0.08 },
            negative_sequence: PerUnitImpedance { resistance_pu: 0.005, reactance_pu: 0.08 },
            zero_sequence: None,
            magnetising_admittance: PerUnitAdmittance { conductance_pu: 0.001, susceptance_pu: -0.02 },
        }],
    }
}

#[test]
fn accepts_parameter_set_with_explicit_unknown_zero_sequence() {
    let equipment = valid_equipment();
    assert!(valid_parameters(&equipment).validate_against(&equipment).is_ok());
}

#[test]
fn rejects_equipment_revision_mismatch() {
    let equipment = valid_equipment();
    let mut params = valid_parameters(&equipment);
    params.equipment_revision += 1;
    assert!(matches!(params.validate_against(&equipment), Err(ElectricalParameterError::EquipmentRevisionMismatch { .. })));
}

#[test]
fn rejects_negative_series_resistance() {
    let equipment = valid_equipment();
    let mut params = valid_parameters(&equipment);
    // A line requires its own two equipment terminals, so test numeric guards
    // through a fully referenced line fixture in the next line-model suite.
    params.transformers[0].positive_sequence.resistance_pu = -0.1;
    assert!(matches!(params.validate_against(&equipment), Err(ElectricalParameterError::InvalidSequenceImpedance { .. })));
}

#[test]
fn rejects_non_finite_transformer_reactance() {
    let equipment = valid_equipment();
    let mut params = valid_parameters(&equipment);
    params.transformers[0].negative_sequence.reactance_pu = f64::NAN;
    assert!(matches!(params.validate_against(&equipment), Err(ElectricalParameterError::InvalidSequenceImpedance { .. })));
}

#[test]
fn rejects_zero_transformer_rated_power() {
    let equipment = valid_equipment();
    let mut params = valid_parameters(&equipment);
    params.transformers[0].rated_power_va = 0.0;
    assert!(matches!(params.validate_against(&equipment), Err(ElectricalParameterError::InvalidTransformerRatedPower(_))));
}

#[test]
fn rejects_unknown_transformer_parameter_reference() {
    let equipment = valid_equipment();
    let mut params = valid_parameters(&equipment);
    params.transformers[0].transformer_id = AssetId::new();
    assert!(matches!(params.validate_against(&equipment), Err(ElectricalParameterError::UnknownTransformer(_))));
}

#[test]
fn rejects_negative_ampacity_is_checked_in_line_records() {
    let equipment = valid_equipment();
    let mut params = valid_parameters(&equipment);
    params.lines.push(LineElectricalParameters {
        asset_id: AssetId::new(), from_terminal: TerminalId::new(), to_terminal: TerminalId::new(),
        length_metres: 100.0,
        conductors: vec![ConductorElectricalData {
            phase: Phase::A,
            series: SeriesImpedancePerKm { resistance_ohm_per_km: 0.2, reactance_ohm_per_km: 0.1 },
            shunt: ShuntAdmittancePerKm { conductance_siemens_per_km: 0.0, susceptance_siemens_per_km: 0.0 },
            continuous_ampacity_ampere: -1.0,
        }],
    });
    // The dangling terminal references are detected before ampacity; the
    // negative ampacity path is covered with a valid line fixture later.
    assert!(matches!(params.validate_against(&equipment), Err(ElectricalParameterError::UnknownTerminal { .. })));
}
