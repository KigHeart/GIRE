//! Deterministic, phase-aware feeder topology reconstruction.
//!
//! This crate builds a connectivity snapshot from an explicit engineering
//! model and switch-position evidence. It does not infer missing connections,
//! estimate switch positions, or issue control commands.
//!
//! Unknown switch positions are excluded from confirmed connectivity and
//! surfaced as uncertainty. They are never treated as open or closed.

use gire_domain::{AssetId, FeederId, Phase, SwitchPosition, TerminalId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Conductors reconstructed independently by the initial topology engine.
const CONDUCTORS: [Phase; 4] = [Phase::A, Phase::B, Phase::C, Phase::Neutral];

/// A network terminal. A terminal may participate in any subset of conductors.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NetworkTerminal {
    pub id: TerminalId,
    pub asset_id: AssetId,
    pub phases: Vec<Phase>,
}

/// Engineering classification for a connection between two terminals.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchKind {
    Line,
    Transformer,
    Switch,
    BusTie,
    Other,
}

/// A two-terminal connectivity element.
///
/// switch_position must be present for a switch and absent for every other
/// branch kind. The topology engine treats a non-switch branch as connected
/// in the supplied model; equipment availability and protection status are
/// separate operational dimensions and are not inferred here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NetworkBranch {
    pub asset_id: AssetId,
    pub from_terminal: TerminalId,
    pub to_terminal: TerminalId,
    pub phases: Vec<Phase>,
    pub kind: BranchKind,
    pub switch_position: Option<SwitchPosition>,
}

/// Immutable input model for one feeder topology revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FeederTopology {
    pub feeder_id: FeederId,
    pub revision: u64,
    pub terminals: Vec<NetworkTerminal>,
    pub branches: Vec<NetworkBranch>,
}

/// Connectivity state of an individual modelled branch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchConnectivity {
    ConfirmedConnected,
    ConfirmedOpen,
    Unknown,
}

/// Whether confirmed connectivity is radial, meshed, or uncertain.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Radiality {
    Radial,
    Meshed,
    Indeterminate,
}

/// Connected components for one conductor, using only confirmed-closed branches.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhaseConnectivity {
    pub phase: Phase,
    pub components: Vec<Vec<TerminalId>>,
    pub radiality: Radiality,
    pub confirmed_open_branches: Vec<AssetId>,
    pub unknown_branches: Vec<AssetId>,
    pub confirmed_cycle_branches: Vec<AssetId>,
}

/// Deterministic result of reconstructing a feeder model.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TopologySnapshot {
    pub feeder_id: FeederId,
    pub revision: u64,
    pub terminal_count: usize,
    pub branch_count: usize,
    pub phases: Vec<PhaseConnectivity>,
}

impl TopologySnapshot {
    /// Reconstructs conductor connectivity after validating the complete input.
    pub fn build(model: &FeederTopology) -> Result<Self, TopologyError> {
        validate_model(model)?;

        let terminals: BTreeMap<TerminalId, &NetworkTerminal> = model
            .terminals
            .iter()
            .map(|terminal| (terminal.id, terminal))
            .collect();

        let mut phases = Vec::with_capacity(CONDUCTORS.len());
        for phase in CONDUCTORS {
            phases.push(build_phase(phase, &terminals, &model.branches));
        }

        Ok(Self {
            feeder_id: model.feeder_id,
            revision: model.revision,
            terminal_count: model.terminals.len(),
            branch_count: model.branches.len(),
            phases,
        })
    }

    /// Returns the computed summary for a conductor, if it was reconstructed.
    pub fn phase(&self, phase: Phase) -> Option<&PhaseConnectivity> {
        self.phases.iter().find(|summary| summary.phase == phase)
    }
}

fn validate_model(model: &FeederTopology) -> Result<(), TopologyError> {
    if model.revision == 0 {
        return Err(TopologyError::ZeroRevision);
    }

    let mut terminal_ids = BTreeSet::new();
    for terminal in &model.terminals {
        if !terminal_ids.insert(terminal.id) {
            return Err(TopologyError::DuplicateTerminal(terminal.id));
        }
        if terminal.phases.is_empty() {
            return Err(TopologyError::TerminalHasNoPhases(terminal.id));
        }
        let mut seen = BTreeSet::new();
        for phase in &terminal.phases {
            if !seen.insert(*phase) {
                return Err(TopologyError::DuplicateTerminalPhase {
                    terminal: terminal.id,
                    phase: *phase,
                });
            }
        }
    }

    let terminal_map: BTreeMap<TerminalId, &NetworkTerminal> = model
        .terminals
        .iter()
        .map(|terminal| (terminal.id, terminal))
        .collect();
    let mut branch_ids = BTreeSet::new();

    for branch in &model.branches {
        if !branch_ids.insert(branch.asset_id) {
            return Err(TopologyError::DuplicateBranch(branch.asset_id));
        }
        if branch.from_terminal == branch.to_terminal {
            return Err(TopologyError::SelfLoop(branch.asset_id));
        }

        let from =
            terminal_map
                .get(&branch.from_terminal)
                .ok_or(TopologyError::UnknownTerminal {
                    branch: branch.asset_id,
                    terminal: branch.from_terminal,
                })?;
        let to = terminal_map
            .get(&branch.to_terminal)
            .ok_or(TopologyError::UnknownTerminal {
                branch: branch.asset_id,
                terminal: branch.to_terminal,
            })?;

        if branch.phases.is_empty() {
            return Err(TopologyError::BranchHasNoPhases(branch.asset_id));
        }
        let mut seen = BTreeSet::new();
        for phase in &branch.phases {
            if !seen.insert(*phase) {
                return Err(TopologyError::DuplicateBranchPhase {
                    branch: branch.asset_id,
                    phase: *phase,
                });
            }
            if !from.phases.contains(phase) || !to.phases.contains(phase) {
                return Err(TopologyError::PhaseNotPresentAtBothTerminals {
                    branch: branch.asset_id,
                    phase: *phase,
                });
            }
        }

        match (branch.kind, branch.switch_position) {
            (BranchKind::Switch, None) => {
                return Err(TopologyError::SwitchPositionRequired(branch.asset_id));
            }
            (kind, Some(_)) if kind != BranchKind::Switch => {
                return Err(TopologyError::UnexpectedSwitchPosition(branch.asset_id));
            }
            _ => {}
        }
    }

    Ok(())
}

fn branch_connectivity(branch: &NetworkBranch) -> BranchConnectivity {
    match branch.switch_position {
        Some(SwitchPosition::Open) => BranchConnectivity::ConfirmedOpen,
        Some(SwitchPosition::Closed) | None => BranchConnectivity::ConfirmedConnected,
        Some(SwitchPosition::Unknown(_)) => BranchConnectivity::Unknown,
    }
}

fn build_phase(
    phase: Phase,
    terminals: &BTreeMap<TerminalId, &NetworkTerminal>,
    branches: &[NetworkBranch],
) -> PhaseConnectivity {
    let phase_terminals: Vec<TerminalId> = terminals
        .iter()
        .filter_map(|(id, terminal)| terminal.phases.contains(&phase).then_some(*id))
        .collect();

    let mut index = BTreeMap::new();
    for (position, terminal_id) in phase_terminals.iter().enumerate() {
        index.insert(*terminal_id, position);
    }
    let mut sets = DisjointSet::new(phase_terminals.len());
    let mut confirmed_open_branches = Vec::new();
    let mut unknown_branches = Vec::new();
    let mut confirmed_cycle_branches = Vec::new();

    // Sort by stable asset identity so diagnostics do not depend on input order.
    let mut phase_branches: Vec<&NetworkBranch> = branches
        .iter()
        .filter(|branch| branch.phases.contains(&phase))
        .collect();
    phase_branches.sort_by_key(|branch| branch.asset_id);

    for branch in phase_branches {
        match branch_connectivity(branch) {
            BranchConnectivity::ConfirmedConnected => {
                let from = index[&branch.from_terminal];
                let to = index[&branch.to_terminal];
                if !sets.union(from, to) {
                    confirmed_cycle_branches.push(branch.asset_id);
                }
            }
            BranchConnectivity::ConfirmedOpen => confirmed_open_branches.push(branch.asset_id),
            BranchConnectivity::Unknown => unknown_branches.push(branch.asset_id),
        }
    }

    let mut grouped: BTreeMap<usize, Vec<TerminalId>> = BTreeMap::new();
    for (position, terminal_id) in phase_terminals.iter().enumerate() {
        let root = sets.find(position);
        grouped.entry(root).or_default().push(*terminal_id);
    }
    let mut components: Vec<Vec<TerminalId>> = grouped.into_values().collect();
    for component in &mut components {
        component.sort();
    }
    components.sort_by_key(|component| component[0]);

    let radiality = if !confirmed_cycle_branches.is_empty() {
        Radiality::Meshed
    } else if !unknown_branches.is_empty() {
        Radiality::Indeterminate
    } else {
        Radiality::Radial
    };

    PhaseConnectivity {
        phase,
        components,
        radiality,
        confirmed_open_branches,
        unknown_branches,
        confirmed_cycle_branches,
    }
}

/// Union-find with path compression and union by rank.
struct DisjointSet {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl DisjointSet {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, item: usize) -> usize {
        if self.parent[item] != item {
            self.parent[item] = self.find(self.parent[item]);
        }
        self.parent[item]
    }

    /// Returns false when both vertices were already connected (a cycle edge).
    fn union(&mut self, left: usize, right: usize) -> bool {
        let mut left_root = self.find(left);
        let mut right_root = self.find(right);
        if left_root == right_root {
            return false;
        }
        if self.rank[left_root] < self.rank[right_root] {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        self.parent[right_root] = left_root;
        if self.rank[left_root] == self.rank[right_root] {
            self.rank[left_root] += 1;
        }
        true
    }
}

/// Input-model validation failures. No partial snapshot is returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopologyError {
    ZeroRevision,
    DuplicateTerminal(TerminalId),
    TerminalHasNoPhases(TerminalId),
    DuplicateTerminalPhase {
        terminal: TerminalId,
        phase: Phase,
    },
    DuplicateBranch(AssetId),
    SelfLoop(AssetId),
    UnknownTerminal {
        branch: AssetId,
        terminal: TerminalId,
    },
    BranchHasNoPhases(AssetId),
    DuplicateBranchPhase {
        branch: AssetId,
        phase: Phase,
    },
    PhaseNotPresentAtBothTerminals {
        branch: AssetId,
        phase: Phase,
    },
    SwitchPositionRequired(AssetId),
    UnexpectedSwitchPosition(AssetId),
}

impl fmt::Display for TopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroRevision => f.write_str("topology revision must be non-zero"),
            Self::DuplicateTerminal(id) => write!(f, "duplicate terminal identifier: {id}"),
            Self::TerminalHasNoPhases(id) => write!(f, "terminal {id} has no phases"),
            Self::DuplicateTerminalPhase { terminal, phase } => {
                write!(f, "terminal {terminal} repeats phase {phase:?}")
            }
            Self::DuplicateBranch(id) => write!(f, "duplicate branch asset identifier: {id}"),
            Self::SelfLoop(id) => write!(f, "branch {id} connects a terminal to itself"),
            Self::UnknownTerminal { branch, terminal } => {
                write!(f, "branch {branch} references unknown terminal {terminal}")
            }
            Self::BranchHasNoPhases(id) => write!(f, "branch {id} has no phases"),
            Self::DuplicateBranchPhase { branch, phase } => {
                write!(f, "branch {branch} repeats phase {phase:?}")
            }
            Self::PhaseNotPresentAtBothTerminals { branch, phase } => {
                write!(
                    f,
                    "branch {branch} phase {phase:?} is not present at both terminals"
                )
            }
            Self::SwitchPositionRequired(id) => {
                write!(f, "switch branch {id} requires an explicit switch position")
            }
            Self::UnexpectedSwitchPosition(id) => {
                write!(f, "non-switch branch {id} cannot carry a switch position")
            }
        }
    }
}

impl std::error::Error for TopologyError {}
