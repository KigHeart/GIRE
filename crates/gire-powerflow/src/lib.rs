//! Deterministic backward/forward-sweep kernel for a single-phase radial feeder.
//!
//! This narrow kernel accepts an assembled single-phase series-impedance tree.
//! It does not infer topology, validate upstream evidence, model shunts,
//! mutual coupling, transformers, neutral return, or issue equipment controls.
//! Callers must perform those checks before invoking the solver.
use gire_domain::TerminalId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

/// Complex rectangular value. Solver quantities use SI units.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Complex64 {
    pub re: f64,
    pub im: f64,
}

impl Complex64 {
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
    pub fn magnitude(self) -> f64 {
        self.re.hypot(self.im)
    }
    pub fn conjugate(self) -> Self {
        Self::new(self.re, -self.im)
    }
    pub fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
}
impl std::ops::Add for Complex64 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.re + rhs.re, self.im + rhs.im)
    }
}
impl std::ops::Sub for Complex64 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.re - rhs.re, self.im - rhs.im)
    }
}
impl std::ops::Mul for Complex64 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::new(
            self.re * rhs.re - self.im * rhs.im,
            self.re * rhs.im + self.im * rhs.re,
        )
    }
}
impl std::ops::Div for Complex64 {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        let d = rhs.re * rhs.re + rhs.im * rhs.im;
        Self::new(
            (self.re * rhs.re + self.im * rhs.im) / d,
            (self.im * rhs.re - self.re * rhs.im) / d,
        )
    }
}
impl std::ops::Neg for Complex64 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.im)
    }
}

/// Constant-P/Q demand. Positive P and Q denote consumption.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConstantPowerLoad {
    pub terminal_id: TerminalId,
    pub active_power_watts: f64,
    pub reactive_power_vars: f64,
}

/// Series-only branch; impedance is in ohms and ampacity in RMS amperes.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct SeriesBranch {
    pub asset_id: gire_domain::AssetId,
    pub from_terminal: TerminalId,
    pub to_terminal: TerminalId,
    pub resistance_ohms: f64,
    pub reactance_ohms: f64,
    pub continuous_ampacity_amperes: f64,
}

/// One-phase radial network with one ideal slack/reference terminal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RadialFeeder {
    pub slack_terminal: TerminalId,
    /// RMS line-to-neutral voltage phasor in volts.
    pub slack_voltage_volts: Complex64,
    pub branches: Vec<SeriesBranch>,
    pub loads: Vec<ConstantPowerLoad>,
}

/// Absolute RMS phasor-delta tolerance in volts.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct SolverOptions {
    pub voltage_tolerance_volts: f64,
    pub maximum_iterations: usize,
}
impl Default for SolverOptions {
    fn default() -> Self {
        Self {
            voltage_tolerance_volts: 1.0e-8,
            maximum_iterations: 200,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerminalResult {
    pub terminal_id: TerminalId,
    pub voltage_volts: Complex64,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BranchResult {
    pub asset_id: gire_domain::AssetId,
    /// Current follows declared from_terminal -> to_terminal orientation.
    pub current_amperes: Complex64,
    pub apparent_power_from_va: Complex64,
    pub apparent_power_to_va: Complex64,
    pub copper_loss_watts: f64,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PowerFlowResult {
    pub iterations: usize,
    pub maximum_voltage_delta_volts: f64,
    pub terminals: Vec<TerminalResult>,
    pub branches: Vec<BranchResult>,
}
impl PowerFlowResult {
    pub fn terminal(&self, id: TerminalId) -> Option<&TerminalResult> {
        self.terminals.iter().find(|item| item.terminal_id == id)
    }
}

/// Solve a single-phase, single-slack, radial constant-P/Q network by
/// backward/forward sweep. This function is not an operational control path.
pub fn solve_radial(
    feeder: &RadialFeeder,
    options: SolverOptions,
) -> Result<PowerFlowResult, PowerFlowError> {
    validate_inputs(feeder, options)?;
    let mut nodes = BTreeSet::from([feeder.slack_terminal]);
    for branch in &feeder.branches {
        nodes.insert(branch.from_terminal);
        nodes.insert(branch.to_terminal);
    }
    for load in &feeder.loads {
        nodes.insert(load.terminal_id);
    }

    let mut adjacency: BTreeMap<TerminalId, Vec<(TerminalId, usize)>> = BTreeMap::new();
    for (index, branch) in feeder.branches.iter().enumerate() {
        adjacency
            .entry(branch.from_terminal)
            .or_default()
            .push((branch.to_terminal, index));
        adjacency
            .entry(branch.to_terminal)
            .or_default()
            .push((branch.from_terminal, index));
    }
    for neighbours in adjacency.values_mut() {
        neighbours.sort_by_key(|(terminal, _)| *terminal);
    }

    let mut parent: BTreeMap<TerminalId, (TerminalId, usize)> = BTreeMap::new();
    let mut order = Vec::with_capacity(nodes.len());
    let mut visited = BTreeSet::from([feeder.slack_terminal]);
    let mut queue = VecDeque::from([feeder.slack_terminal]);
    while let Some(current) = queue.pop_front() {
        order.push(current);
        if let Some(neighbours) = adjacency.get(&current) {
            for (next, branch_index) in neighbours {
                if visited.insert(*next) {
                    parent.insert(*next, (current, *branch_index));
                    queue.push_back(*next);
                }
            }
        }
    }
    if visited != nodes {
        return Err(PowerFlowError::DisconnectedNetwork);
    }
    if feeder.branches.len() != nodes.len() - 1 {
        return Err(PowerFlowError::NotRadialTree);
    }
    let mut children: BTreeMap<TerminalId, Vec<TerminalId>> = BTreeMap::new();
    for (child, (upstream, _)) in &parent {
        children.entry(*upstream).or_default().push(*child);
    }
    let mut loads_by_node: BTreeMap<TerminalId, Complex64> = BTreeMap::new();
    for load in &feeder.loads {
        let entry = loads_by_node.entry(load.terminal_id).or_default();
        *entry = *entry + Complex64::new(load.active_power_watts, load.reactive_power_vars);
    }

    let mut voltages: BTreeMap<TerminalId, Complex64> = nodes
        .iter()
        .map(|id| (*id, feeder.slack_voltage_volts))
        .collect();
    let mut final_branch_currents = vec![Complex64::default(); feeder.branches.len()];
    let mut final_delta = f64::INFINITY;
    let mut converged_iteration = 0;
    for iteration in 1..=options.maximum_iterations {
        let mut downstream: BTreeMap<TerminalId, Complex64> = BTreeMap::new();
        for terminal in order.iter().rev().copied() {
            let voltage = voltages[&terminal];
            if voltage.magnitude() <= f64::MIN_POSITIVE {
                return Err(PowerFlowError::VoltageCollapsed(terminal));
            }
            let power = loads_by_node.get(&terminal).copied().unwrap_or_default();
            let load_current = (power / voltage).conjugate();
            let child_current = children
                .get(&terminal)
                .into_iter()
                .flatten()
                .map(|child| downstream[child])
                .fold(Complex64::default(), |sum, value| sum + value);
            downstream.insert(terminal, load_current + child_current);
        }
        let mut next_voltages =
            BTreeMap::from([(feeder.slack_terminal, feeder.slack_voltage_volts)]);
        let mut branch_currents = vec![Complex64::default(); feeder.branches.len()];
        for terminal in order.iter().copied().skip(1) {
            let (upstream, branch_index) = parent[&terminal];
            let branch = feeder.branches[branch_index];
            let current = downstream[&terminal];
            let z = Complex64::new(branch.resistance_ohms, branch.reactance_ohms);
            let next_voltage = next_voltages[&upstream] - z * current;
            if !next_voltage.is_finite() || next_voltage.magnitude() <= f64::MIN_POSITIVE {
                return Err(PowerFlowError::VoltageCollapsed(terminal));
            }
            branch_currents[branch_index] = if branch.from_terminal == upstream {
                current
            } else {
                -current
            };
            next_voltages.insert(terminal, next_voltage);
        }
        final_delta = nodes
            .iter()
            .map(|id| (next_voltages[id] - voltages[id]).magnitude())
            .fold(0.0_f64, f64::max);
        voltages = next_voltages;
        final_branch_currents = branch_currents;
        if final_delta <= options.voltage_tolerance_volts {
            converged_iteration = iteration;
            break;
        }
    }
    if converged_iteration == 0 {
        return Err(PowerFlowError::DidNotConverge {
            iterations: options.maximum_iterations,
            final_delta_volts: final_delta,
        });
    }

    let terminals = order
        .iter()
        .map(|id| TerminalResult {
            terminal_id: *id,
            voltage_volts: voltages[id],
        })
        .collect();
    let mut branches = Vec::with_capacity(feeder.branches.len());
    for (index, branch) in feeder.branches.iter().enumerate() {
        let current = final_branch_currents[index];
        let from_v = voltages[&branch.from_terminal];
        let to_v = voltages[&branch.to_terminal];
        branches.push(BranchResult {
            asset_id: branch.asset_id,
            current_amperes: current,
            apparent_power_from_va: from_v * current.conjugate(),
            apparent_power_to_va: to_v * (-current).conjugate(),
            copper_loss_watts: current.magnitude().powi(2) * branch.resistance_ohms,
        });
    }
    Ok(PowerFlowResult {
        iterations: converged_iteration,
        maximum_voltage_delta_volts: final_delta,
        terminals,
        branches,
    })
}

fn validate_inputs(feeder: &RadialFeeder, options: SolverOptions) -> Result<(), PowerFlowError> {
    if !feeder.slack_voltage_volts.is_finite() || feeder.slack_voltage_volts.magnitude() <= 0.0 {
        return Err(PowerFlowError::InvalidSlackVoltage);
    }
    if !options.voltage_tolerance_volts.is_finite()
        || options.voltage_tolerance_volts <= 0.0
        || options.maximum_iterations == 0
    {
        return Err(PowerFlowError::InvalidSolverOptions);
    }
    let mut branch_ids = BTreeSet::new();
    let mut edges = BTreeSet::new();
    for branch in &feeder.branches {
        if branch.from_terminal == branch.to_terminal {
            return Err(PowerFlowError::SelfLoop(branch.asset_id));
        }
        if !branch_ids.insert(branch.asset_id) {
            return Err(PowerFlowError::DuplicateBranch(branch.asset_id));
        }
        let edge = if branch.from_terminal < branch.to_terminal {
            (branch.from_terminal, branch.to_terminal)
        } else {
            (branch.to_terminal, branch.from_terminal)
        };
        if !edges.insert(edge) {
            return Err(PowerFlowError::DuplicateEdge);
        }
        if !branch.resistance_ohms.is_finite()
            || branch.resistance_ohms < 0.0
            || !branch.reactance_ohms.is_finite()
            || branch.resistance_ohms.hypot(branch.reactance_ohms) <= 0.0
            || !branch.continuous_ampacity_amperes.is_finite()
            || branch.continuous_ampacity_amperes <= 0.0
        {
            return Err(PowerFlowError::InvalidBranchParameters(branch.asset_id));
        }
    }
    for load in &feeder.loads {
        if !load.active_power_watts.is_finite() || !load.reactive_power_vars.is_finite() {
            return Err(PowerFlowError::InvalidLoad(load.terminal_id));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PowerFlowError {
    InvalidSlackVoltage,
    InvalidSolverOptions,
    SelfLoop(gire_domain::AssetId),
    DuplicateBranch(gire_domain::AssetId),
    DuplicateEdge,
    InvalidBranchParameters(gire_domain::AssetId),
    InvalidLoad(TerminalId),
    NotRadialTree,
    DisconnectedNetwork,
    VoltageCollapsed(TerminalId),
    DidNotConverge {
        iterations: usize,
        final_delta_volts: f64,
    },
}
impl fmt::Display for PowerFlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "power-flow calculation rejected or failed: {self:?}")
    }
}
impl std::error::Error for PowerFlowError {}
