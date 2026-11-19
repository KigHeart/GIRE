use gire_domain::AssetId;
use gire_electrical::{ComplexImpedanceOhms, ElectricalParameterError, PhaseDomainImpedanceMatrix};

fn valid_four_wire() -> PhaseDomainImpedanceMatrix {
    let z = ComplexImpedanceOhms {
        resistance_ohms: 0.1,
        reactance_ohms: 0.2,
    };
    let mut entries = [[None; 4]; 4];
    for row in 0..4 {
        for column in 0..4 {
            entries[row][column] = Some(if row == column {
                z
            } else {
                ComplexImpedanceOhms {
                    resistance_ohms: 0.01,
                    reactance_ohms: 0.03,
                }
            });
        }
    }
    PhaseDomainImpedanceMatrix {
        conductor_present: [true; 4],
        entries,
        continuous_ampacity_amperes: [Some(300.0), Some(300.0), Some(300.0), Some(100.0)],
    }
}

#[test]
fn accepts_complete_reciprocal_four_wire_matrix() {
    let asset = AssetId::new();
    assert!(valid_four_wire().validate(asset).is_ok());
}

#[test]
fn rejects_missing_mutual_impedance_instead_of_defaulting_to_zero() {
    let asset = AssetId::new();
    let mut matrix = valid_four_wire();
    matrix.entries[0][1] = None;
    assert!(matches!(
        matrix.validate(asset),
        Err(ElectricalParameterError::MissingPhaseDomainEntry {
            equipment,
            row: 0,
            column: 1
        }) if equipment == asset
    ));
}

#[test]
fn rejects_non_reciprocal_complex_impedance() {
    let asset = AssetId::new();
    let mut matrix = valid_four_wire();
    matrix.entries[1][0] = Some(ComplexImpedanceOhms {
        resistance_ohms: 0.02,
        reactance_ohms: 0.03,
    });
    assert!(matches!(
        matrix.validate(asset),
        Err(ElectricalParameterError::NonReciprocalPhaseDomainMatrix { .. })
    ));
}

#[test]
fn rejects_matrix_entries_for_absent_conductors() {
    let asset = AssetId::new();
    let mut matrix = valid_four_wire();
    matrix.conductor_present[3] = false;
    matrix.continuous_ampacity_amperes[3] = None;
    for index in 0..4 {
        matrix.entries[3][index] = None;
        matrix.entries[index][3] = None;
    }
    matrix.entries[0][3] = Some(ComplexImpedanceOhms {
        resistance_ohms: 0.0,
        reactance_ohms: 0.0,
    });
    assert!(matches!(
        matrix.validate(asset),
        Err(ElectricalParameterError::EntryForAbsentConductor { .. })
    ));
}

#[test]
fn requires_ampacity_for_every_present_conductor_including_neutral() {
    let asset = AssetId::new();
    let mut matrix = valid_four_wire();
    matrix.continuous_ampacity_amperes[3] = None;
    assert!(matches!(
        matrix.validate(asset),
        Err(ElectricalParameterError::InvalidPhaseDomainAmpacity {
            equipment,
            conductor: 3
        }) if equipment == asset
    ));
}
