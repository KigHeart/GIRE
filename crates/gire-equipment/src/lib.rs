//! Explicit electrical equipment model and referential-integrity validation.
//!
//! Transformer windings are not collapsed into same-phase graph edges:
//! connection type, rated voltage, vector-group clock and phase correspondence
//! remain explicit. This is a data model, not a power-flow solver.

use gire_domain::{AssetId, Phase, TerminalId};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, fmt};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Substation { pub id: AssetId, pub name: String }

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VoltageLevel {
    pub id: AssetId,
    pub substation_id: AssetId,
    /// Nominal line-to-line voltage in volts.
    pub nominal_voltage_volts: f64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EquipmentTerminal {
    pub id: TerminalId,
    pub equipment_id: AssetId,
    pub voltage_level_id: AssetId,
    pub phases: Vec<Phase>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Busbar {
    pub id: AssetId,
    pub substation_id: AssetId,
    pub voltage_level_id: AssetId,
    pub terminal_id: TerminalId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindingConnection { Wye, WyeGrounded, Delta, ZigZag }

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TransformerWinding {
    pub id: AssetId,
    pub terminal_id: TerminalId,
    pub voltage_level_id: AssetId,
    /// Winding rated line-to-line voltage in volts.
    pub rated_voltage_volts: f64,
    pub connection: WindingConnection,
    pub phases: Vec<Phase>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhaseMapping { pub primary: Phase, pub secondary: Phase }

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PowerTransformer {
    pub id: AssetId,
    pub substation_id: AssetId,
    /// IEC vector-group clock number, 0 through 11.
    pub vector_group_clock_hour: u8,
    pub primary: TransformerWinding,
    pub secondary: TransformerWinding,
    pub phase_mapping: Vec<PhaseMapping>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EquipmentModel {
    pub revision: u64,
    pub substations: Vec<Substation>,
    pub voltage_levels: Vec<VoltageLevel>,
    pub terminals: Vec<EquipmentTerminal>,
    pub busbars: Vec<Busbar>,
    pub transformers: Vec<PowerTransformer>,
}

impl EquipmentModel {
    /// Validates references and basic electrical invariants without repairing
    /// or defaulting missing data.
    pub fn validate(&self) -> Result<(), EquipmentError> {
        if self.revision == 0 { return Err(EquipmentError::ZeroRevision); }

        let mut asset_ids = BTreeSet::new();
        let mut substation_ids = BTreeSet::new();
        for item in &self.substations {
            if !asset_ids.insert(item.id) || !substation_ids.insert(item.id) {
                return Err(EquipmentError::DuplicateAsset(item.id));
            }
            if item.name.trim().is_empty() { return Err(EquipmentError::EmptyName(item.id)); }
        }

        let mut voltage_ids = BTreeSet::new();
        for level in &self.voltage_levels {
            if !asset_ids.insert(level.id) || !voltage_ids.insert(level.id) {
                return Err(EquipmentError::DuplicateAsset(level.id));
            }
            if !level.nominal_voltage_volts.is_finite() || level.nominal_voltage_volts <= 0.0 {
                return Err(EquipmentError::InvalidNominalVoltage(level.id));
            }
            if !substation_ids.contains(&level.substation_id) {
                return Err(EquipmentError::UnknownSubstation { asset: level.id, substation: level.substation_id });
            }
        }

        let mut terminal_ids = BTreeSet::new();
        for terminal in &self.terminals {
            if !terminal_ids.insert(terminal.id) { return Err(EquipmentError::DuplicateTerminal(terminal.id)); }
            if !voltage_ids.contains(&terminal.voltage_level_id) {
                return Err(EquipmentError::UnknownVoltageLevel { terminal: terminal.id, voltage_level: terminal.voltage_level_id });
            }
            validate_phases(terminal.equipment_id, &terminal.phases)?;
        }
        let terminal_by_id: BTreeMap<_, _> = self.terminals.iter().map(|t| (t.id, t)).collect();
        let voltage_by_id: BTreeMap<_, _> = self.voltage_levels.iter().map(|v| (v.id, v)).collect();

        for busbar in &self.busbars {
            if !asset_ids.insert(busbar.id) { return Err(EquipmentError::DuplicateAsset(busbar.id)); }
            let level = voltage_by_id.get(&busbar.voltage_level_id)
                .ok_or(EquipmentError::UnknownBusbarVoltageLevel { busbar: busbar.id, voltage_level: busbar.voltage_level_id })?;
            if level.substation_id != busbar.substation_id { return Err(EquipmentError::SubstationMismatch(busbar.id)); }
            let terminal = terminal_by_id.get(&busbar.terminal_id)
                .ok_or(EquipmentError::UnknownEquipmentTerminal(busbar.terminal_id))?;
            if terminal.equipment_id != busbar.id || terminal.voltage_level_id != busbar.voltage_level_id {
                return Err(EquipmentError::TerminalOwnershipMismatch(busbar.id));
            }
        }

        for transformer in &self.transformers {
            if !asset_ids.insert(transformer.id) { return Err(EquipmentError::DuplicateAsset(transformer.id)); }
            if transformer.vector_group_clock_hour > 11 {
                return Err(EquipmentError::InvalidVectorGroupClock(transformer.id));
            }
            if !substation_ids.contains(&transformer.substation_id) {
                return Err(EquipmentError::UnknownSubstation { asset: transformer.id, substation: transformer.substation_id });
            }
            for winding in [&transformer.primary, &transformer.secondary] {
                if !asset_ids.insert(winding.id) { return Err(EquipmentError::DuplicateAsset(winding.id)); }
                if !winding.rated_voltage_volts.is_finite() || winding.rated_voltage_volts <= 0.0 {
                    return Err(EquipmentError::InvalidWindingVoltage(winding.id));
                }
                let terminal = terminal_by_id.get(&winding.terminal_id)
                    .ok_or(EquipmentError::UnknownEquipmentTerminal(winding.terminal_id))?;
                if terminal.equipment_id != transformer.id
                    || terminal.voltage_level_id != winding.voltage_level_id
                    || terminal.phases != winding.phases {
                    return Err(EquipmentError::WindingTerminalMismatch(winding.id));
                }
                let level = voltage_by_id.get(&winding.voltage_level_id)
                    .ok_or(EquipmentError::UnknownVoltageLevel { terminal: winding.terminal_id, voltage_level: winding.voltage_level_id })?;
                if level.substation_id != transformer.substation_id {
                    return Err(EquipmentError::SubstationMismatch(transformer.id));
                }
                validate_phases(winding.id, &winding.phases)?;
            }
            validate_phase_mapping(transformer)?;
        }
        Ok(())
    }
}

fn validate_phases(asset: AssetId, phases: &[Phase]) -> Result<(), EquipmentError> {
    if phases.is_empty() { return Err(EquipmentError::NoPhases(asset)); }
    let mut seen = BTreeSet::new();
    for phase in phases {
        if !seen.insert(*phase) { return Err(EquipmentError::DuplicatePhase { asset, phase: *phase }); }
    }
    Ok(())
}

fn validate_phase_mapping(transformer: &PowerTransformer) -> Result<(), EquipmentError> {
    let mut primary_seen = BTreeSet::new();
    let mut secondary_seen = BTreeSet::new();
    for mapping in &transformer.phase_mapping {
        if !matches!(mapping.primary, Phase::A | Phase::B | Phase::C)
            || !matches!(mapping.secondary, Phase::A | Phase::B | Phase::C)
            || !transformer.primary.phases.contains(&mapping.primary)
            || !transformer.secondary.phases.contains(&mapping.secondary)
            || !primary_seen.insert(mapping.primary)
            || !secondary_seen.insert(mapping.secondary) {
            return Err(EquipmentError::InvalidPhaseMapping(transformer.id));
        }
    }
    let primary_count = transformer.primary.phases.iter().filter(|p| matches!(p, Phase::A | Phase::B | Phase::C)).count();
    let secondary_count = transformer.secondary.phases.iter().filter(|p| matches!(p, Phase::A | Phase::B | Phase::C)).count();
    if transformer.phase_mapping.len() != primary_count || transformer.phase_mapping.len() != secondary_count {
        return Err(EquipmentError::IncompletePhaseMapping(transformer.id));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EquipmentError {
    ZeroRevision,
    DuplicateAsset(AssetId),
    DuplicateTerminal(TerminalId),
    EmptyName(AssetId),
    InvalidNominalVoltage(AssetId),
    InvalidWindingVoltage(AssetId),
    InvalidVectorGroupClock(AssetId),
    UnknownSubstation { asset: AssetId, substation: AssetId },
    UnknownVoltageLevel { terminal: TerminalId, voltage_level: AssetId },
    UnknownBusbarVoltageLevel { busbar: AssetId, voltage_level: AssetId },
    UnknownEquipmentTerminal(TerminalId),
    SubstationMismatch(AssetId),
    TerminalOwnershipMismatch(AssetId),
    WindingTerminalMismatch(AssetId),
    NoPhases(AssetId),
    DuplicatePhase { asset: AssetId, phase: Phase },
    InvalidPhaseMapping(AssetId),
    IncompletePhaseMapping(AssetId),
}
impl fmt::Display for EquipmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "invalid equipment model: {self:?}") }
}
impl std::error::Error for EquipmentError {}
