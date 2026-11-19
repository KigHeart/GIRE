use gire_domain::{AssetId, TerminalId};
use gire_powerflow::{solve_radial, Complex64, ConstantPowerLoad, PowerFlowError, RadialFeeder, SeriesBranch, SolverOptions};
fn id<T: Default>() -> T { T::default() }
fn branch(from_terminal: TerminalId, to_terminal: TerminalId, resistance_ohms: f64, reactance_ohms: f64) -> SeriesBranch {
    SeriesBranch { asset_id: id::<AssetId>(), from_terminal, to_terminal, resistance_ohms, reactance_ohms, continuous_ampacity_amperes: 100.0 }
}
fn feeder(slack_terminal: TerminalId, voltage: f64, branches: Vec<SeriesBranch>, loads: Vec<ConstantPowerLoad>) -> RadialFeeder {
    RadialFeeder { slack_terminal, slack_voltage_volts: Complex64::new(voltage, 0.0), branches, loads }
}
#[test]
fn no_load_feeder_preserves_slack_voltage() {
    let source = id::<TerminalId>(); let remote = id::<TerminalId>();
    let result = solve_radial(&feeder(source, 230.0, vec![branch(source, remote, 0.2, 0.1)], vec![]), SolverOptions::default()).unwrap();
    assert_eq!(result.terminal(remote).unwrap().voltage_volts, Complex64::new(230.0, 0.0));
    assert_eq!(result.branches[0].current_amperes, Complex64::default());
}
#[test]
fn one_branch_resistive_load_has_consistent_current_and_loss() {
    let source = id::<TerminalId>(); let node = id::<TerminalId>();
    let model = feeder(source, 230.0, vec![branch(source, node, 0.1, 0.0)], vec![ConstantPowerLoad { terminal_id: node, active_power_watts: 1_000.0, reactive_power_vars: 0.0 }]);
    let result = solve_radial(&model, SolverOptions::default()).unwrap();
    let voltage = result.terminal(node).unwrap().voltage_volts.re;
    let current = result.branches[0].current_amperes.magnitude();
    assert!(voltage < 230.0 && voltage > 229.0);
    assert!((current - 1_000.0 / voltage).abs() < 1.0e-7);
    assert!((result.branches[0].copper_loss_watts - current * current * 0.1).abs() < 1.0e-9);
}
#[test]
fn two_branch_feeder_includes_downstream_load_current() {
    let source = id::<TerminalId>(); let middle = id::<TerminalId>(); let end = id::<TerminalId>();
    let model = feeder(source, 230.0, vec![branch(source, middle, 0.1, 0.0), branch(middle, end, 0.1, 0.0)], vec![
        ConstantPowerLoad { terminal_id: middle, active_power_watts: 500.0, reactive_power_vars: 0.0 },
        ConstantPowerLoad { terminal_id: end, active_power_watts: 500.0, reactive_power_vars: 0.0 },
    ]);
    let result = solve_radial(&model, SolverOptions::default()).unwrap();
    assert!(result.terminal(end).unwrap().voltage_volts.re < result.terminal(middle).unwrap().voltage_volts.re);
    assert!(result.branches[0].current_amperes.magnitude() > result.branches[1].current_amperes.magnitude());
}
#[test]
fn meshed_network_is_rejected() {
    let a = id::<TerminalId>(); let b = id::<TerminalId>(); let c = id::<TerminalId>();
    let model = feeder(a, 230.0, vec![branch(a,b,0.1,0.0), branch(b,c,0.1,0.0), branch(c,a,0.1,0.0)], vec![]);
    assert_eq!(solve_radial(&model, SolverOptions::default()), Err(PowerFlowError::NotRadialTree));
}
#[test]
fn disconnected_network_is_rejected() {
    let a = id::<TerminalId>(); let b = id::<TerminalId>(); let c = id::<TerminalId>(); let d = id::<TerminalId>();
    let model = feeder(a, 230.0, vec![branch(a,b,0.1,0.0), branch(c,d,0.1,0.0)], vec![]);
    assert_eq!(solve_radial(&model, SolverOptions::default()), Err(PowerFlowError::DisconnectedNetwork));
}
#[test]
fn zero_impedance_is_rejected() {
    let a = id::<TerminalId>(); let b = id::<TerminalId>();
    let model = feeder(a, 230.0, vec![branch(a,b,0.0,0.0)], vec![]);
    assert!(matches!(solve_radial(&model, SolverOptions::default()), Err(PowerFlowError::InvalidBranchParameters(_))));
}
