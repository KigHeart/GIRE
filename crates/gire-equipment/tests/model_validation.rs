use gire_domain::{AssetId, Phase, TerminalId};
use gire_equipment::{
    Busbar, EquipmentError, EquipmentModel, EquipmentTerminal, PhaseMapping, PowerTransformer,
    Substation, TransformerWinding, VoltageLevel, WindingConnection,
};

fn id() -> AssetId {
    AssetId::new()
}

fn terminal() -> TerminalId {
    TerminalId::new()
}

fn valid_model() -> EquipmentModel {
    let sub = id();
    let hv = id();
    let lv = id();
    let transformer_id = id();
    let hv_terminal = terminal();
    let lv_terminal = terminal();
    let busbar_id = id();
    let busbar_terminal = terminal();
    let phases = vec![Phase::A, Phase::B, Phase::C];

    EquipmentModel {
        revision: 1,
        substations: vec![Substation {
            id: sub,
            name: "Substation 1".into(),
        }],
        voltage_levels: vec![
            VoltageLevel {
                id: hv,
                substation_id: sub,
                nominal_voltage_volts: 33_000.0,
            },
            VoltageLevel {
                id: lv,
                substation_id: sub,
                nominal_voltage_volts: 11_000.0,
            },
        ],
        terminals: vec![
            EquipmentTerminal {
                id: hv_terminal,
                equipment_id: transformer_id,
                voltage_level_id: hv,
                phases: phases.clone(),
            },
            EquipmentTerminal {
                id: lv_terminal,
                equipment_id: transformer_id,
                voltage_level_id: lv,
                phases: phases.clone(),
            },
            EquipmentTerminal {
                id: busbar_terminal,
                equipment_id: busbar_id,
                voltage_level_id: lv,
                phases: phases.clone(),
            },
        ],
        busbars: vec![Busbar {
            id: busbar_id,
            substation_id: sub,
            voltage_level_id: lv,
            terminal_id: busbar_terminal,
        }],
        transformers: vec![PowerTransformer {
            id: transformer_id,
            substation_id: sub,
            vector_group_clock_hour: 11,
            primary: TransformerWinding {
                id: id(),
                terminal_id: hv_terminal,
                voltage_level_id: hv,
                rated_voltage_volts: 33_000.0,
                connection: WindingConnection::WyeGrounded,
                phases: phases.clone(),
            },
            secondary: TransformerWinding {
                id: id(),
                terminal_id: lv_terminal,
                voltage_level_id: lv,
                rated_voltage_volts: 11_000.0,
                connection: WindingConnection::Delta,
                phases: phases.clone(),
            },
            phase_mapping: vec![
                PhaseMapping {
                    primary: Phase::A,
                    secondary: Phase::A,
                },
                PhaseMapping {
                    primary: Phase::B,
                    secondary: Phase::B,
                },
                PhaseMapping {
                    primary: Phase::C,
                    secondary: Phase::C,
                },
            ],
        }],
    }
}

#[test]
fn accepts_explicit_two_winding_transformer_model() {
    assert!(valid_model().validate().is_ok());
}

#[test]
fn rejects_zero_nominal_voltage() {
    let mut model = valid_model();
    model.voltage_levels[0].nominal_voltage_volts = 0.0;
    assert!(matches!(
        model.validate(),
        Err(EquipmentError::InvalidNominalVoltage(_))
    ));
}

#[test]
fn rejects_vector_group_clock_outside_iec_range() {
    let mut model = valid_model();
    model.transformers[0].vector_group_clock_hour = 12;
    assert!(matches!(
        model.validate(),
        Err(EquipmentError::InvalidVectorGroupClock(_))
    ));
}

#[test]
fn rejects_incomplete_phase_mapping() {
    let mut model = valid_model();
    model.transformers[0].phase_mapping.pop();
    assert!(matches!(
        model.validate(),
        Err(EquipmentError::IncompletePhaseMapping(_))
    ));
}

#[test]
fn rejects_non_bijective_phase_mapping() {
    let mut model = valid_model();
    model.transformers[0].phase_mapping[1].secondary = Phase::A;
    assert!(matches!(
        model.validate(),
        Err(EquipmentError::InvalidPhaseMapping(_))
    ));
}

#[test]
fn rejects_winding_terminal_voltage_level_mismatch() {
    let mut model = valid_model();
    model.transformers[0].secondary.voltage_level_id = model.voltage_levels[0].id;
    assert!(matches!(
        model.validate(),
        Err(EquipmentError::WindingTerminalMismatch(_))
    ));
}

#[test]
fn rejects_terminal_reference_to_unknown_voltage_level() {
    let mut model = valid_model();
    model.terminals[0].voltage_level_id = id();
    assert!(matches!(
        model.validate(),
        Err(EquipmentError::UnknownVoltageLevel { .. })
    ));
}

#[test]
fn rejects_zero_revision() {
    let mut model = valid_model();
    model.revision = 0;
    assert_eq!(model.validate(), Err(EquipmentError::ZeroRevision));
}
