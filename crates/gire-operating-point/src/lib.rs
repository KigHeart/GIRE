//! Version-pinned operating conditions for deterministic grid analysis.
//!
//! This crate records source references, phase-specific constant-P/Q loads,
//! and explicit transformer tap-changer states. It does not solve a network,
//! infer missing values, or issue equipment commands.

use gire_domain::{AssetId, EvidenceId, MeasurementQuality, Phase, SourceId, TerminalId};
use gire_equipment::EquipmentModel;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindingSide {
    Primary,
    Secondary,
}

/// One phase of an ideal reference/source bus. Voltage is RMS line-to-neutral
/// magnitude in volts; angle is electrical degrees relative to the declared
/// system reference. The reference frame must be consistent across sources.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SourcePhase {
    pub source_id: SourceId,
    pub terminal_id: TerminalId,
    pub phase: Phase,
    pub voltage_magnitude_volts_rms: f64,
    pub voltage_angle_degrees: f64,
    pub quality: MeasurementQuality,
    pub observed_at_unix_ms: u64,
    pub evidence_id: EvidenceId,
}

/// A constant complex-power demand on one phase. Positive P and Q mean
/// consumption; negative values represent explicit net export/injection.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LoadPhase {
    pub load_id: AssetId,
    pub terminal_id: TerminalId,
    pub phase: Phase,
    pub active_power_watts: f64,
    pub reactive_power_vars: f64,
    pub quality: MeasurementQuality,
    pub observed_at_unix_ms: u64,
    pub evidence_id: EvidenceId,
}

/// Explicit tap-changer capability and observed position. Step voltage is
/// percent of the controlled winding rated voltage per tap step.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TransformerTapState {
    pub transformer_id: AssetId,
    pub winding_side: WindingSide,
    pub minimum_position: i32,
    pub neutral_position: i32,
    pub maximum_position: i32,
    pub current_position: i32,
    pub step_voltage_percent: f64,
    pub quality: MeasurementQuality,
    pub observed_at_unix_ms: u64,
    pub evidence_id: EvidenceId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OperatingPoint {
    pub revision: u64,
    pub as_of_unix_ms: u64,
    pub topology_revision: u64,
    pub equipment_revision: u64,
    pub electrical_parameter_revision: u64,
    pub sources: Vec<SourcePhase>,
    pub loads: Vec<LoadPhase>,
    pub transformer_taps: Vec<TransformerTapState>,
}

impl OperatingPoint {
    /// Checks referential, numeric, revision and provenance invariants.
    /// Suspect/estimated values are retained but make the point ineligible
    /// for deterministic solver execution.
    pub fn validate_against(&self, equipment: &EquipmentModel) -> Result<(), OperatingPointError> {
        equipment.validate().map_err(|_| OperatingPointError::InvalidEquipmentModel)?;
        if self.revision == 0 || self.topology_revision == 0
            || self.equipment_revision == 0 || self.electrical_parameter_revision == 0 {
            return Err(OperatingPointError::ZeroRevision);
        }
        if self.equipment_revision != equipment.revision {
            return Err(OperatingPointError::EquipmentRevisionMismatch {
                operating_point: self.equipment_revision,
                equipment: equipment.revision,
            });
        }

        let mut source_keys = BTreeSet::new();
        for source in &self.sources {
            validate_terminal_phase(equipment, source.terminal_id, source.phase)?;
            if !source_keys.insert((source.source_id, source.phase)) {
                return Err(OperatingPointError::DuplicateSourcePhase(source.source_id, source.phase));
            }
            if !source.voltage_magnitude_volts_rms.is_finite()
                || source.voltage_magnitude_volts_rms <= 0.0
                || !source.voltage_angle_degrees.is_finite()
            {
                return Err(OperatingPointError::InvalidSourceVoltage(source.source_id, source.phase));
            }
            validate_observation(source.quality, source.observed_at_unix_ms, source.evidence_id)?;
            if source.observed_at_unix_ms > self.as_of_unix_ms { return Err(OperatingPointError::ObservationAfterSnapshot); }
        }

        let mut load_keys = BTreeSet::new();
        for load in &self.loads {
            validate_terminal_phase(equipment, load.terminal_id, load.phase)?;
            if !load_keys.insert((load.load_id, load.phase)) {
                return Err(OperatingPointError::DuplicateLoadPhase(load.load_id, load.phase));
            }
            if !load.active_power_watts.is_finite() || !load.reactive_power_vars.is_finite() {
                return Err(OperatingPointError::InvalidLoadPower(load.load_id, load.phase));
            }
            validate_observation(load.quality, load.observed_at_unix_ms, load.evidence_id)?;
            if load.observed_at_unix_ms > self.as_of_unix_ms { return Err(OperatingPointError::ObservationAfterSnapshot); }
        }

        let mut tap_ids: Vec<(AssetId, WindingSide)> = Vec::new();
        for tap in &self.transformer_taps {
            let transformer = equipment.transformers.iter()
                .find(|item| item.id == tap.transformer_id)
                .ok_or(OperatingPointError::UnknownTransformer(tap.transformer_id))?;
            if tap_ids.contains(&(tap.transformer_id, tap.winding_side)) {
                return Err(OperatingPointError::DuplicateTapState(tap.transformer_id, tap.winding_side));
            }
            tap_ids.push((tap.transformer_id, tap.winding_side));
            if tap.minimum_position > tap.neutral_position
                || tap.neutral_position > tap.maximum_position
                || tap.current_position < tap.minimum_position
                || tap.current_position > tap.maximum_position
                || !tap.step_voltage_percent.is_finite()
                || tap.step_voltage_percent <= 0.0
            {
                return Err(OperatingPointError::InvalidTapRange(tap.transformer_id));
            }
            let winding = match tap.winding_side {
                WindingSide::Primary => &transformer.primary,
                WindingSide::Secondary => &transformer.secondary,
            };
            if !winding.rated_voltage_volts.is_finite() || winding.rated_voltage_volts <= 0.0 {
                return Err(OperatingPointError::InvalidTapWinding(tap.transformer_id));
            }
            validate_observation(tap.quality, tap.observed_at_unix_ms, tap.evidence_id)?;
            if tap.observed_at_unix_ms > self.as_of_unix_ms { return Err(OperatingPointError::ObservationAfterSnapshot); }
        }
        Ok(())
    }

    /// True only when every supplied operational observation has Good quality.
    /// Empty collections are not sufficient: at least one source is required.
    pub fn is_solver_eligible(&self) -> bool {
        !self.sources.is_empty()
            && self.sources.iter().all(|v| v.quality == MeasurementQuality::Good)
            && self.loads.iter().all(|v| v.quality == MeasurementQuality::Good)
            && self.transformer_taps.iter().all(|v| v.quality == MeasurementQuality::Good)
    }
}

fn validate_terminal_phase(
    equipment: &EquipmentModel,
    terminal_id: TerminalId,
    phase: Phase,
) -> Result<(), OperatingPointError> {
    let terminal = equipment.terminals.iter().find(|item| item.id == terminal_id)
        .ok_or(OperatingPointError::UnknownTerminal(terminal_id))?;
    if !matches!(phase, Phase::A | Phase::B | Phase::C) || !terminal.phases.contains(&phase) {
        return Err(OperatingPointError::PhaseNotPresent { terminal: terminal_id, phase });
    }
    Ok(())
}

fn validate_observation(
    quality: MeasurementQuality,
    observed_at_unix_ms: u64,
    evidence_id: EvidenceId,
) -> Result<(), OperatingPointError> {
    if observed_at_unix_ms == 0 || evidence_id.as_uuid().is_nil() {
        return Err(OperatingPointError::InvalidObservationProvenance);
    }
    if matches!(quality, MeasurementQuality::Bad | MeasurementQuality::Stale | MeasurementQuality::Missing) {
        return Err(OperatingPointError::UnusableObservation(quality));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatingPointError {
    InvalidEquipmentModel,
    ZeroRevision,
    EquipmentRevisionMismatch { operating_point: u64, equipment: u64 },
    UnknownTerminal(TerminalId),
    PhaseNotPresent { terminal: TerminalId, phase: Phase },
    DuplicateSourcePhase(SourceId, Phase),
    InvalidSourceVoltage(SourceId, Phase),
    DuplicateLoadPhase(AssetId, Phase),
    InvalidLoadPower(AssetId, Phase),
    UnknownTransformer(AssetId),
    DuplicateTapState(AssetId, WindingSide),
    InvalidTapRange(AssetId),
    InvalidTapWinding(AssetId),
    UnusableObservation(MeasurementQuality),
    InvalidObservationProvenance,
    ObservationAfterSnapshot,
}

impl fmt::Display for OperatingPointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid operating point: {self:?}")
    }
}

impl std::error::Error for OperatingPointError {}
