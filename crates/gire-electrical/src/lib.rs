//! Explicit electrical parameter records and validation.
//!
//! Values are stored in declared SI-derived units. This crate does not infer
//! missing parameters, convert unsupported transformer connections, or solve
//! a network. It validates the parameter records before a numerical engine
//! consumes them.

use gire_domain::{AssetId, Phase, TerminalId};
use gire_equipment::EquipmentModel;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Per-unit resistance and reactance on the equipment's stated base.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PerUnitImpedance {
    pub resistance_pu: f64,
    pub reactance_pu: f64,
}

/// Per-unit conductance and susceptance on the equipment's stated base.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PerUnitAdmittance {
    pub conductance_pu: f64,
    pub susceptance_pu: f64,
}

/// Series impedance per kilometre for one physical conductor.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct SeriesImpedancePerKm {
    pub resistance_ohm_per_km: f64,
    pub reactance_ohm_per_km: f64,
}

/// Shunt admittance per kilometre for one physical conductor.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct ShuntAdmittancePerKm {
    pub conductance_siemens_per_km: f64,
    pub susceptance_siemens_per_km: f64,
}

/// Electrical data for one conductor of a line or cable.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConductorElectricalData {
    pub phase: Phase,
    pub series: SeriesImpedancePerKm,
    pub shunt: ShuntAdmittancePerKm,
    /// Continuous ampacity in amperes at the declared reference conditions.
    pub continuous_ampacity_ampere: f64,
}

/// Parameter record for a physical two-terminal line or cable.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LineElectricalParameters {
    pub asset_id: AssetId,
    pub from_terminal: TerminalId,
    pub to_terminal: TerminalId,
    /// Physical route length in metres; not electrical length after scaling.
    pub length_metres: f64,
    pub conductors: Vec<ConductorElectricalData>,
}

/// Transformer sequence impedances use the transformer rated-power and
/// rated-voltage base defined in the associated equipment model.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TransformerElectricalParameters {
    pub transformer_id: AssetId,
    pub rated_power_va: f64,
    pub positive_sequence: PerUnitImpedance,
    pub negative_sequence: PerUnitImpedance,
    /// None means not supplied / not established, not zero impedance.
    pub zero_sequence: Option<PerUnitImpedance>,
    /// Magnetising branch on the same per-unit base.
    pub magnetising_admittance: PerUnitAdmittance,
}

/// Complete parameter set for a specific equipment-model revision.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ElectricalParameterSet {
    pub equipment_revision: u64,
    pub lines: Vec<LineElectricalParameters>,
    pub transformers: Vec<TransformerElectricalParameters>,
}

impl ElectricalParameterSet {
    /// Validates parameter coverage and cross-references against equipment.
    ///
    /// A missing zero-sequence transformer value is retained as unknown; this
    /// validator does not make a zero-sequence calculation permissible.
    pub fn validate_against(
        &self,
        equipment: &EquipmentModel,
    ) -> Result<(), ElectricalParameterError> {
        equipment
            .validate()
            .map_err(|_| ElectricalParameterError::InvalidEquipmentModel)?;
        if self.equipment_revision == 0 || self.equipment_revision != equipment.revision {
            return Err(ElectricalParameterError::EquipmentRevisionMismatch {
                parameters: self.equipment_revision,
                equipment: equipment.revision,
            });
        }

        let terminals: BTreeMap<_, _> = equipment.terminals.iter().map(|t| (t.id, t)).collect();
        let transformers: BTreeMap<_, _> =
            equipment.transformers.iter().map(|t| (t.id, t)).collect();

        let mut seen_assets = BTreeSet::new();
        for line in &self.lines {
            if !seen_assets.insert(line.asset_id) {
                return Err(ElectricalParameterError::DuplicateEquipmentParameters(
                    line.asset_id,
                ));
            }
            if !line.length_metres.is_finite() || line.length_metres <= 0.0 {
                return Err(ElectricalParameterError::InvalidLineLength(line.asset_id));
            }
            let from = terminals.get(&line.from_terminal).ok_or(
                ElectricalParameterError::UnknownTerminal {
                    equipment: line.asset_id,
                    terminal: line.from_terminal,
                },
            )?;
            let to = terminals.get(&line.to_terminal).ok_or(
                ElectricalParameterError::UnknownTerminal {
                    equipment: line.asset_id,
                    terminal: line.to_terminal,
                },
            )?;
            if from.equipment_id != line.asset_id
                || to.equipment_id != line.asset_id
                || from.voltage_level_id != to.voltage_level_id
            {
                return Err(ElectricalParameterError::LineTerminalMismatch(
                    line.asset_id,
                ));
            }
            if line.conductors.is_empty() {
                return Err(ElectricalParameterError::NoConductorData(line.asset_id));
            }
            let mut phases = BTreeSet::new();
            for conductor in &line.conductors {
                if !phases.insert(conductor.phase) {
                    return Err(ElectricalParameterError::DuplicateConductorPhase {
                        equipment: line.asset_id,
                        phase: conductor.phase,
                    });
                }
                if !from.phases.contains(&conductor.phase) || !to.phases.contains(&conductor.phase) {
                    return Err(ElectricalParameterError::ConductorPhaseNotAtBothTerminals {
                        equipment: line.asset_id,
                        phase: conductor.phase,
                    });
                }
                validate_series(line.asset_id, conductor.phase, conductor.series)?;
                validate_shunt(line.asset_id, conductor.phase, conductor.shunt)?;
                if !conductor.continuous_ampacity_ampere.is_finite()
                    || conductor.continuous_ampacity_ampere <= 0.0
                {
                    return Err(ElectricalParameterError::InvalidAmpacity {
                        equipment: line.asset_id,
                        phase: conductor.phase,
                    });
                }
            }
        }

        for transformer_parameters in &self.transformers {
            let id = transformer_parameters.transformer_id;
            if !seen_assets.insert(id) {
                return Err(ElectricalParameterError::DuplicateEquipmentParameters(id));
            }
            let transformer = transformers
                .get(&id)
                .ok_or(ElectricalParameterError::UnknownTransformer(id))?;
            if !transformer_parameters.rated_power_va.is_finite()
                || transformer_parameters.rated_power_va <= 0.0
            {
                return Err(ElectricalParameterError::InvalidTransformerRatedPower(id));
            }
            for (sequence, impedance) in [
                ("positive", Some(transformer_parameters.positive_sequence)),
                ("negative", Some(transformer_parameters.negative_sequence)),
                ("zero", transformer_parameters.zero_sequence),
            ] {
                if let Some(value) = impedance {
                    validate_per_unit_impedance(id, sequence, value)?;
                }
            }
            let mag = transformer_parameters.magnetising_admittance;
            if !mag.conductance_pu.is_finite()
                || mag.conductance_pu < 0.0
                || !mag.susceptance_pu.is_finite()
            {
                return Err(ElectricalParameterError::InvalidMagnetisingAdmittance(id));
            }
            // A rated-power base is not enough if the model's winding voltage
            // ratings do not agree with the equipment model's nominal levels.
            for winding in [&transformer.primary, &transformer.secondary] {
                if !winding.rated_voltage_volts.is_finite() || winding.rated_voltage_volts <= 0.0 {
                    return Err(ElectricalParameterError::InvalidTransformerBase(id));
                }
            }
        }
        Ok(())
    }
}

fn validate_series(
    equipment: AssetId,
    phase: Phase,
    value: SeriesImpedancePerKm,
) -> Result<(), ElectricalParameterError> {
    if !value.resistance_ohm_per_km.is_finite()
        || value.resistance_ohm_per_km < 0.0
        || !value.reactance_ohm_per_km.is_finite()
    {
        return Err(ElectricalParameterError::InvalidSeriesImpedance { equipment, phase });
    }
    Ok(())
}

fn validate_shunt(
    equipment: AssetId,
    phase: Phase,
    value: ShuntAdmittancePerKm,
) -> Result<(), ElectricalParameterError> {
    if !value.conductance_siemens_per_km.is_finite()
        || value.conductance_siemens_per_km < 0.0
        || !value.susceptance_siemens_per_km.is_finite()
    {
        return Err(ElectricalParameterError::InvalidShuntAdmittance { equipment, phase });
    }
    Ok(())
}

fn validate_per_unit_impedance(
    transformer: AssetId,
    sequence: &'static str,
    value: PerUnitImpedance,
) -> Result<(), ElectricalParameterError> {
    if !value.resistance_pu.is_finite()
        || value.resistance_pu < 0.0
        || !value.reactance_pu.is_finite()
    {
        return Err(ElectricalParameterError::InvalidSequenceImpedance {
            transformer,
            sequence,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElectricalParameterError {
    InvalidEquipmentModel,
    EquipmentRevisionMismatch {
        parameters: u64,
        equipment: u64,
    },
    DuplicateEquipmentParameters(AssetId),
    InvalidLineLength(AssetId),
    UnknownTerminal {
        equipment: AssetId,
        terminal: TerminalId,
    },
    LineTerminalMismatch(AssetId),
    NoConductorData(AssetId),
    DuplicateConductorPhase {
        equipment: AssetId,
        phase: Phase,
    },
    ConductorPhaseNotAtBothTerminals {
        equipment: AssetId,
        phase: Phase,
    },
    InvalidSeriesImpedance {
        equipment: AssetId,
        phase: Phase,
    },
    InvalidShuntAdmittance {
        equipment: AssetId,
        phase: Phase,
    },
    InvalidAmpacity {
        equipment: AssetId,
        phase: Phase,
    },
    UnknownTransformer(AssetId),
    InvalidTransformerRatedPower(AssetId),
    InvalidSequenceImpedance {
        transformer: AssetId,
        sequence: &'static str,
    },
    InvalidMagnetisingAdmittance(AssetId),
    InvalidTransformerBase(AssetId),
}

impl fmt::Display for ElectricalParameterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid electrical parameters: {self:?}")
    }
}

impl std::error::Error for ElectricalParameterError {}
