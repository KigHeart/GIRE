use gire_domain::{AssetId, TerminalId};
use gire_powerflow::{solve_radial, Complex64, ConstantPowerLoad, RadialFeeder, SeriesBranch, SolverOptions};

fn feeder(ampacity: f64, watts: f64) -> RadialFeeder {
    let source = TerminalId::new();
    let load = TerminalId::new();
    RadialFeeder {
        slack_terminal: source,
        slack_voltage_volts: Complex64::new(230.0, 0.0),
        branches: vec![SeriesBranch {
            asset_id: AssetId::new(), from_terminal: source, to_terminal: load,
            resistance_ohms: 0.1, reactance_ohms: 0.0,
            continuous_ampacity_amperes: ampacity,
        }],
        loads: vec![ConstantPowerLoad { terminal_id: load, active_power_watts: watts, reactive_power_vars: 0.0 }],
    }
}
#[test]
fn over_ampacity_result_is_explicitly_flagged() {
    let result = solve_radial(&feeder(1.0, 2_000.0), SolverOptions::default()).unwrap();
    assert!(result.has_ampacity_violations());
    assert!(result.branches[0].ampacity_exceeded);
    assert!(result.branches[0].loading_percent > 100.0);
}
#[test]
fn below_ampacity_result_is_not_flagged() {
    let result = solve_radial(&feeder(100.0, 500.0), SolverOptions::default()).unwrap();
    assert!(!result.has_ampacity_violations());
    assert!(!result.branches[0].ampacity_exceeded);
}
