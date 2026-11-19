//! Validated conversion from version-pinned network models to the narrow radial solver.
//!
//! Only one confirmed, single-phase radial component made entirely of
//! series-only lines is accepted. This is advisory analysis, never a switching
//! authority.

use crate::{
    Complex64, ConstantPowerLoad, PowerFlowError, PowerFlowResult, RadialFeeder, SeriesBranch,
    SolverOptions, solve_radial,
};
use gire_domain::{AssetId, FeederId, Phase, SwitchPosition, TerminalId};
use gire_electrical::ElectricalParameterSet;
use gire_equipment::EquipmentModel;
use gire_network_validation::{NetworkValidationError, validate_network};
use gire_operating_point::OperatingPoint;
use gire_topology::{BranchKind, FeederTopology, Radiality, TopologySnapshot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PowerFlowProvenance {
    pub feeder_id: FeederId,
    pub phase: Phase,
    pub topology_revision: u64,
    pub equipment_revision: u64,
    pub electrical_parameter_revision: u64,
    pub operating_point_revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedRadialFeeder {
    feeder: RadialFeeder,
    pub provenance: PowerFlowProvenance,
}

impl PreparedRadialFeeder {
    pub fn feeder(&self) -> &RadialFeeder {
        &self.feeder
    }

    pub fn solve(&self, options: SolverOptions) -> Result<ValidatedPowerFlowResult, AdapterError> {
        let result = solve_radial(&self.feeder, options).map_err(AdapterError::Solver)?;
        Ok(ValidatedPowerFlowResult {
            provenance: self.provenance,
            result,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedPowerFlowResult {
    pub provenance: PowerFlowProvenance,
    pub result: PowerFlowResult,
}

impl ValidatedPowerFlowResult {
    pub fn has_ampacity_violations(&self) -> bool {
        self.result.has_ampacity_violations()
    }
}

pub fn prepare_radial_feeder(
    topology: &FeederTopology,
    equipment: &EquipmentModel,
    parameters: &ElectricalParameterSet,
    operating_point: &OperatingPoint,
    phase: Phase,
) -> Result<PreparedRadialFeeder, AdapterError> {
    if !matches!(phase, Phase::A | Phase::B | Phase::C) {
        return Err(AdapterError::UnsupportedPhase(phase));
    }

    let report = validate_network(topology, equipment, parameters, operating_point)
        .map_err(AdapterError::NetworkValidation)?;
    if !report.is_solver_eligible() {
        return Err(AdapterError::NetworkNotSolverEligible);
    }

    let snapshot = TopologySnapshot::build(topology).map_err(|_| AdapterError::InvalidTopology)?;
    let summary = snapshot
        .phase(phase)
        .ok_or(AdapterError::MissingPhase(phase))?;
    if summary.radiality != Radiality::Radial {
        return Err(AdapterError::PhaseNotConfirmedRadial(phase));
    }

    let sources: Vec<_> = operating_point
        .sources
        .iter()
        .filter(|source| source.phase == phase)
        .collect();
    let source = match sources.as_slice() {
        [] => return Err(AdapterError::MissingSource(phase)),
        [source] => *source,
        _ => return Err(AdapterError::MultipleSources(phase)),
    };

    let component = summary
        .components
        .iter()
        .find(|members| members.contains(&source.terminal_id))
        .ok_or(AdapterError::SourceOutsideTopology(source.terminal_id))?;

    let loads: Vec<_> = operating_point
        .loads
        .iter()
        .filter(|load| load.phase == phase)
        .collect();
    if let Some(load) = loads
        .iter()
        .find(|load| !component.contains(&load.terminal_id))
    {
        return Err(AdapterError::LoadOutsideSourceComponent(load.terminal_id));
    }

    let mut branches = Vec::new();
    for branch in &topology.branches {
        if !branch.phases.contains(&phase)
            || !component.contains(&branch.from_terminal)
            || !component.contains(&branch.to_terminal)
        {
            continue;
        }

        if matches!(
            branch.switch_position,
            Some(SwitchPosition::Open | SwitchPosition::Unknown(_))
        ) {
            return Err(AdapterError::UnconfirmedBranchState(branch.asset_id));
        }
        if branch.kind != BranchKind::Line {
            return Err(AdapterError::UnsupportedBranchKind(branch.asset_id));
        }

        let line = parameters
            .lines
            .iter()
            .find(|item| item.asset_id == branch.asset_id)
            .ok_or(AdapterError::MissingLineParameters(branch.asset_id))?;
        let conductor = line
            .conductors
            .iter()
            .find(|item| item.phase == phase)
            .ok_or(AdapterError::MissingConductorParameters(
                branch.asset_id,
                phase,
            ))?;

        if conductor.shunt.conductance_siemens_per_km != 0.0
            || conductor.shunt.susceptance_siemens_per_km != 0.0
        {
            return Err(AdapterError::UnsupportedShunt(branch.asset_id));
        }

        let length_km = line.length_metres / 1000.0;
        branches.push(SeriesBranch {
            asset_id: branch.asset_id,
            from_terminal: branch.from_terminal,
            to_terminal: branch.to_terminal,
            resistance_ohms: conductor.series.resistance_ohm_per_km * length_km,
            reactance_ohms: conductor.series.reactance_ohm_per_km * length_km,
            continuous_ampacity_amperes: conductor.continuous_ampacity_ampere,
        });
    }

    let angle = source.voltage_angle_degrees.to_radians();
    let feeder = RadialFeeder {
        slack_terminal: source.terminal_id,
        slack_voltage_volts: Complex64::new(
            source.voltage_magnitude_volts_rms * angle.cos(),
            source.voltage_magnitude_volts_rms * angle.sin(),
        ),
        branches,
        loads: loads
            .into_iter()
            .map(|load| ConstantPowerLoad {
                terminal_id: load.terminal_id,
                active_power_watts: load.active_power_watts,
                reactive_power_vars: load.reactive_power_vars,
            })
            .collect(),
    };

    Ok(PreparedRadialFeeder {
        feeder,
        provenance: PowerFlowProvenance {
            feeder_id: topology.feeder_id,
            phase,
            topology_revision: topology.revision,
            equipment_revision: equipment.revision,
            electrical_parameter_revision: parameters.revision,
            operating_point_revision: operating_point.revision,
        },
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AdapterError {
    NetworkValidation(NetworkValidationError),
    NetworkNotSolverEligible,
    InvalidTopology,
    UnsupportedPhase(Phase),
    MissingPhase(Phase),
    PhaseNotConfirmedRadial(Phase),
    MissingSource(Phase),
    MultipleSources(Phase),
    SourceOutsideTopology(TerminalId),
    LoadOutsideSourceComponent(TerminalId),
    UnconfirmedBranchState(AssetId),
    UnsupportedBranchKind(AssetId),
    MissingLineParameters(AssetId),
    MissingConductorParameters(AssetId, Phase),
    UnsupportedShunt(AssetId),
    Solver(PowerFlowError),
}
