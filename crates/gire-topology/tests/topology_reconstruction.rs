use gire_domain::{AssetId, FeederId, Phase, SwitchPosition, TerminalId, UnknownSwitchReason};
use gire_topology::{
    BranchKind, FeederTopology, NetworkBranch, NetworkTerminal, Radiality, TopologyError,
    TopologySnapshot,
};

fn terminal() -> NetworkTerminal {
    NetworkTerminal {
        id: TerminalId::new(),
        asset_id: AssetId::new(),
        phases: vec![Phase::A, Phase::B, Phase::C, Phase::Neutral],
    }
}

fn branch(
    from: TerminalId,
    to: TerminalId,
    kind: BranchKind,
    switch_position: Option<SwitchPosition>,
) -> NetworkBranch {
    NetworkBranch {
        asset_id: AssetId::new(),
        from_terminal: from,
        to_terminal: to,
        phases: vec![Phase::A, Phase::B, Phase::C, Phase::Neutral],
        kind,
        switch_position,
    }
}

fn model(terminals: Vec<NetworkTerminal>, branches: Vec<NetworkBranch>) -> FeederTopology {
    FeederTopology {
        feeder_id: FeederId::new(),
        revision: 1,
        terminals,
        branches,
    }
}

#[test]
fn closed_feeder_reconstructs_one_radial_component_per_conductor() {
    let a = terminal();
    let b = terminal();
    let c = terminal();
    let feeder = model(
        vec![a.clone(), b.clone(), c.clone()],
        vec![
            branch(a.id, b.id, BranchKind::Line, None),
            branch(b.id, c.id, BranchKind::Line, None),
        ],
    );

    let snapshot = TopologySnapshot::build(&feeder).expect("valid topology");
    for phase in [Phase::A, Phase::B, Phase::C, Phase::Neutral] {
        let summary = snapshot.phase(phase).expect("conductor summary");
        assert_eq!(summary.radiality, Radiality::Radial);
        assert_eq!(summary.components.len(), 1);
        assert_eq!(summary.components[0].len(), 3);
    }
}

#[test]
fn open_switch_separates_confirmed_components() {
    let a = terminal();
    let b = terminal();
    let c = terminal();
    let feeder = model(
        vec![a.clone(), b.clone(), c.clone()],
        vec![
            branch(a.id, b.id, BranchKind::Line, None),
            branch(b.id, c.id, BranchKind::Switch, Some(SwitchPosition::Open)),
        ],
    );

    let snapshot = TopologySnapshot::build(&feeder).expect("valid topology");
    let summary = snapshot.phase(Phase::A).expect("phase A");
    assert_eq!(summary.radiality, Radiality::Radial);
    assert_eq!(summary.components.len(), 2);
    assert_eq!(summary.confirmed_open_branches.len(), 1);
}

#[test]
fn unknown_switch_is_not_assumed_open_or_closed() {
    let a = terminal();
    let b = terminal();
    let unknown_switch = branch(
        a.id,
        b.id,
        BranchKind::Switch,
        Some(SwitchPosition::Unknown(UnknownSwitchReason::ConflictingEvidence)),
    );
    let feeder = model(vec![a, b], vec![unknown_switch]);

    let snapshot = TopologySnapshot::build(&feeder).expect("valid topology");
    let summary = snapshot.phase(Phase::A).expect("phase A");
    assert_eq!(summary.radiality, Radiality::Indeterminate);
    assert_eq!(summary.components.len(), 2);
    assert_eq!(summary.unknown_branches.len(), 1);
}

#[test]
fn known_cycle_is_reported_as_meshed() {
    let a = terminal();
    let b = terminal();
    let c = terminal();
    let feeder = model(
        vec![a.clone(), b.clone(), c.clone()],
        vec![
            branch(a.id, b.id, BranchKind::Line, None),
            branch(b.id, c.id, BranchKind::Line, None),
            branch(c.id, a.id, BranchKind::Line, None),
        ],
    );

    let snapshot = TopologySnapshot::build(&feeder).expect("valid topology");
    let summary = snapshot.phase(Phase::A).expect("phase A");
    assert_eq!(summary.radiality, Radiality::Meshed);
    assert_eq!(summary.confirmed_cycle_branches.len(), 1);
}

#[test]
fn unknown_terminal_reference_rejects_entire_model() {
    let a = terminal();
    let missing = TerminalId::new();
    let feeder = model(vec![a.clone()], vec![branch(a.id, missing, BranchKind::Line, None)]);

    assert!(matches!(
        TopologySnapshot::build(&feeder),
        Err(TopologyError::UnknownTerminal { .. })
    ));
}

#[test]
fn switch_without_explicit_position_is_rejected() {
    let a = terminal();
    let b = terminal();
    let feeder = model(
        vec![a.clone(), b.clone()],
        vec![branch(a.id, b.id, BranchKind::Switch, None)],
    );

    assert!(matches!(
        TopologySnapshot::build(&feeder),
        Err(TopologyError::SwitchPositionRequired(_))
    ));
}

#[test]
fn phase_mismatch_between_branch_and_terminal_is_rejected() {
    let a = terminal();
    let mut b = terminal();
    b.phases = vec![Phase::B, Phase::C, Phase::Neutral];
    let mut edge = branch(a.id, b.id, BranchKind::Line, None);
    edge.phases = vec![Phase::A, Phase::B, Phase::C, Phase::Neutral];
    let feeder = model(vec![a, b], vec![edge]);

    assert!(matches!(
        TopologySnapshot::build(&feeder),
        Err(TopologyError::PhaseNotPresentAtBothTerminals { .. })
    ));
}

#[test]
fn isolated_terminals_are_preserved_as_components() {
    let a = terminal();
    let b = terminal();
    let feeder = model(vec![a, b], vec![]);

    let snapshot = TopologySnapshot::build(&feeder).expect("valid topology");
    let summary = snapshot.phase(Phase::A).expect("phase A");
    assert_eq!(summary.components.len(), 2);
    assert!(summary.components.iter().all(|component| component.len() == 1));
}

#[test]
fn zero_revision_is_rejected() {
    let mut feeder = model(vec![terminal()], vec![]);
    feeder.revision = 0;

    assert_eq!(
        TopologySnapshot::build(&feeder).unwrap_err(),
        TopologyError::ZeroRevision
    );
}

#[test]
fn branch_input_order_does_not_change_snapshot() {
    let a = terminal();
    let b = terminal();
    let c = terminal();
    let mut feeder = model(
        vec![a.clone(), b.clone(), c.clone()],
        vec![
            branch(a.id, b.id, BranchKind::Line, None),
            branch(b.id, c.id, BranchKind::Line, None),
            branch(c.id, a.id, BranchKind::Line, None),
        ],
    );
    let expected = TopologySnapshot::build(&feeder).expect("valid topology");
    feeder.branches.reverse();
    let actual = TopologySnapshot::build(&feeder).expect("valid topology");
    assert_eq!(actual, expected);
}
