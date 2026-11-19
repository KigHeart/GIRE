//! Cross-layer validation before numerical grid analysis.
//!
//! This crate reconciles topology, equipment, electrical parameters and
//! operating-point evidence. It does not calculate voltages or issue controls.

use gire_domain::{AssetId, Phase, SwitchPosition, TerminalId};
use gire_electrical::{ElectricalParameterSet, LineElectricalParameters};
use gire_equipment::EquipmentModel;
use gire_operating_point::{OperatingPoint, OperatingPointError};
use gire_topology::{BranchKind, FeederTopology, TopologySnapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadSupplyStatus {
    ConfirmedSupplied,
    IndeterminateAcrossUnknownSwitch,
    NoSourcePath,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LoadSupplyAssessment {
    pub load_id: AssetId,
    pub terminal_id: TerminalId,
    pub phase: Phase,
    pub status: LoadSupplyStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NetworkValidationReport {
    pub topology_revision: u64,
    pub equipment_revision: u64,
    pub electrical_parameter_revision: u64,
    pub operating_point_revision: u64,
    pub observations_solver_eligible: bool,
    pub load_assessments: Vec<LoadSupplyAssessment>,
}

impl NetworkValidationReport {
    pub fn is_solver_eligible(&self) -> bool {
        self.observations_solver_eligible
            && self
                .load_assessments
                .iter()
                .all(|item| item.status == LoadSupplyStatus::ConfirmedSupplied)
    }
}

/// Validate cross-layer identity, revision, parameter coverage and source
/// reachability. Unknown switches are considered only to detect uncertainty;
/// they never establish confirmed supply.
pub fn validate_network(
    topology: &FeederTopology,
    equipment: &EquipmentModel,
    parameters: &ElectricalParameterSet,
    operating_point: &OperatingPoint,
) -> Result<NetworkValidationReport, NetworkValidationError> {
    let snapshot =
        TopologySnapshot::build(topology).map_err(|_| NetworkValidationError::InvalidTopology)?;
    equipment
        .validate()
        .map_err(|_| NetworkValidationError::InvalidEquipment)?;
    if topology.revision != operating_point.topology_revision {
        return Err(NetworkValidationError::TopologyRevisionMismatch {
            topology: topology.revision,
            operating_point: operating_point.topology_revision,
        });
    }
    if topology.revision != snapshot.revision {
        return Err(NetworkValidationError::TopologySnapshotMismatch);
    }
    if operating_point.equipment_revision != equipment.revision {
        return Err(NetworkValidationError::EquipmentRevisionMismatch);
    }
    if operating_point.electrical_parameter_revision != parameters.revision {
        return Err(
            NetworkValidationError::ElectricalParameterRevisionMismatch {
                operating_point: operating_point.electrical_parameter_revision,
                parameters: parameters.revision,
            },
        );
    }
    parameters
        .validate_against(equipment)
        .map_err(|_| NetworkValidationError::InvalidElectricalParameters)?;
    operating_point
        .validate_against(equipment)
        .map_err(|_| NetworkValidationError::InvalidOperatingPoint)?;

    validate_terminal_alignment(topology, equipment)?;
    validate_branch_parameter_coverage(topology, parameters)?;

    // A confirmed island cannot have multiple independent voltage references,
    // even when no load is currently recorded in that island.
    for phase in [Phase::A, Phase::B, Phase::C] {
        if let Some(summary) = snapshot.phase(phase) {
            for component in &summary.components {
                let source_count = operating_point
                    .sources
                    .iter()
                    .filter(|source| {
                        source.phase == phase && component.contains(&source.terminal_id)
                    })
                    .count();
                if source_count > 1 {
                    return Err(NetworkValidationError::MultipleSourcesInConfirmedIsland {
                        terminal: component[0],
                        phase,
                    });
                }
            }
        }
    }

    let terminal_ids: BTreeSet<_> = topology.terminals.iter().map(|t| t.id).collect();
    for source in &operating_point.sources {
        if !terminal_ids.contains(&source.terminal_id) {
            return Err(NetworkValidationError::OperatingPointTerminalNotInTopology(
                source.terminal_id,
            ));
        }
    }
    for load in &operating_point.loads {
        if !terminal_ids.contains(&load.terminal_id) {
            return Err(NetworkValidationError::OperatingPointTerminalNotInTopology(
                load.terminal_id,
            ));
        }
    }

    let mut assessments = Vec::with_capacity(operating_point.loads.len());
    for load in &operating_point.loads {
        let phase_summary = snapshot
            .phase(load.phase)
            .ok_or(NetworkValidationError::MissingPhase(load.phase))?;
        let component = phase_summary
            .components
            .iter()
            .find(|members| members.contains(&load.terminal_id))
            .ok_or(NetworkValidationError::OperatingPointTerminalNotInTopology(
                load.terminal_id,
            ))?;
        let sources_in_component = operating_point
            .sources
            .iter()
            .filter(|source| source.phase == load.phase && component.contains(&source.terminal_id))
            .count();

        let status = if sources_in_component == 1 {
            LoadSupplyStatus::ConfirmedSupplied
        } else if sources_in_component > 1 {
            return Err(NetworkValidationError::MultipleSourcesInConfirmedIsland {
                terminal: load.terminal_id,
                phase: load.phase,
            });
        } else if path_exists_including_unknown(
            topology,
            load.terminal_id,
            operating_point,
            load.phase,
        ) {
            LoadSupplyStatus::IndeterminateAcrossUnknownSwitch
        } else {
            LoadSupplyStatus::NoSourcePath
        };

        assessments.push(LoadSupplyAssessment {
            load_id: load.load_id,
            terminal_id: load.terminal_id,
            phase: load.phase,
            status,
        });
    }
    assessments.sort_by_key(|item| (item.load_id, item.phase));

    Ok(NetworkValidationReport {
        topology_revision: topology.revision,
        equipment_revision: equipment.revision,
        electrical_parameter_revision: parameters.revision,
        operating_point_revision: operating_point.revision,
        observations_solver_eligible: operating_point.is_solver_eligible(),
        load_assessments: assessments,
    })
}

fn validate_terminal_alignment(
    topology: &FeederTopology,
    equipment: &EquipmentModel,
) -> Result<(), NetworkValidationError> {
    let equipment_terminals: BTreeMap<_, _> = equipment
        .terminals
        .iter()
        .map(|terminal| (terminal.id, terminal))
        .collect();
    if topology.terminals.len() > equipment_terminals.len() {
        return Err(NetworkValidationError::TerminalSetMismatch);
    }
    for terminal in &topology.terminals {
        let model = equipment_terminals
            .get(&terminal.id)
            .ok_or(NetworkValidationError::TerminalSetMismatch)?;
        if terminal.asset_id != model.equipment_id {
            return Err(NetworkValidationError::TerminalAssetMismatch(terminal.id));
        }
        let topology_phases: BTreeSet<_> = terminal.phases.iter().copied().collect();
        let equipment_phases: BTreeSet<_> = model.phases.iter().copied().collect();
        if topology_phases != equipment_phases {
            return Err(NetworkValidationError::TerminalPhaseMismatch(terminal.id));
        }
    }
    Ok(())
}

fn validate_branch_parameter_coverage(
    topology: &FeederTopology,
    parameters: &ElectricalParameterSet,
) -> Result<(), NetworkValidationError> {
    for branch in &topology.branches {
        match branch.kind {
            BranchKind::Line => {
                let line = parameters
                    .lines
                    .iter()
                    .find(|line| line.asset_id == branch.asset_id)
                    .ok_or(NetworkValidationError::MissingLineParameters(
                        branch.asset_id,
                    ))?;
                if !same_endpoints(line, branch.from_terminal, branch.to_terminal) {
                    return Err(NetworkValidationError::BranchParameterEndpointMismatch(
                        branch.asset_id,
                    ));
                }
                for phase in &branch.phases {
                    if !line
                        .conductors
                        .iter()
                        .any(|conductor| conductor.phase == *phase)
                    {
                        return Err(NetworkValidationError::MissingLinePhaseParameters {
                            asset: branch.asset_id,
                            phase: *phase,
                        });
                    }
                }
            }
            BranchKind::Transformer => {
                if !parameters
                    .transformers
                    .iter()
                    .any(|item| item.transformer_id == branch.asset_id)
                {
                    return Err(NetworkValidationError::MissingTransformerParameters(
                        branch.asset_id,
                    ));
                }
            }
            BranchKind::Switch | BranchKind::BusTie | BranchKind::Other => {}
        }
    }
    Ok(())
}

fn same_endpoints(line: &LineElectricalParameters, a: TerminalId, b: TerminalId) -> bool {
    (line.from_terminal == a && line.to_terminal == b)
        || (line.from_terminal == b && line.to_terminal == a)
}

fn path_exists_including_unknown(
    topology: &FeederTopology,
    start: TerminalId,
    operating_point: &OperatingPoint,
    phase: Phase,
) -> bool {
    let source_terminals: BTreeSet<_> = operating_point
        .sources
        .iter()
        .filter(|source| source.phase == phase)
        .map(|source| source.terminal_id)
        .collect();
    let mut adjacency: BTreeMap<TerminalId, Vec<TerminalId>> = BTreeMap::new();
    for branch in &topology.branches {
        if !branch.phases.contains(&phase)
            || matches!(branch.switch_position, Some(SwitchPosition::Open))
        {
            continue;
        }
        adjacency
            .entry(branch.from_terminal)
            .or_default()
            .push(branch.to_terminal);
        adjacency
            .entry(branch.to_terminal)
            .or_default()
            .push(branch.from_terminal);
    }
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::from([start]);
    while let Some(current) = queue.pop_front() {
        if !visited.insert(current) {
            continue;
        }
        if source_terminals.contains(&current) {
            return true;
        }
        if let Some(neighbours) = adjacency.get(&current) {
            queue.extend(neighbours.iter().copied());
        }
    }
    false
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkValidationError {
    InvalidTopology,
    InvalidEquipment,
    InvalidElectricalParameters,
    InvalidOperatingPoint,
    TopologyRevisionMismatch {
        topology: u64,
        operating_point: u64,
    },
    TopologySnapshotMismatch,
    EquipmentRevisionMismatch,
    ElectricalParameterRevisionMismatch {
        operating_point: u64,
        parameters: u64,
    },
    TerminalSetMismatch,
    TerminalAssetMismatch(TerminalId),
    TerminalPhaseMismatch(TerminalId),
    OperatingPointTerminalNotInTopology(TerminalId),
    MissingPhase(Phase),
    MissingLineParameters(AssetId),
    BranchParameterEndpointMismatch(AssetId),
    MissingLinePhaseParameters {
        asset: AssetId,
        phase: Phase,
    },
    MissingTransformerParameters(AssetId),
    MultipleSourcesInConfirmedIsland {
        terminal: TerminalId,
        phase: Phase,
    },
}

impl fmt::Display for NetworkValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "network validation failed: {self:?}")
    }
}

impl std::error::Error for NetworkValidationError {}

impl From<OperatingPointError> for NetworkValidationError {
    fn from(_: OperatingPointError) -> Self {
        Self::InvalidOperatingPoint
    }
}
