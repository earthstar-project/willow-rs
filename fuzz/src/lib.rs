use willow_data_model::path::*;

pub fn test_successor<const MCL: usize, const MCC: usize, const MPL: usize>(
    baseline: PathRc<MCL, MCC, MPL>,
    candidate: PathRc<MCL, MCC, MPL>,
    max_path: PathRc<MCL, MCC, MPL>,
) {
    let successor = baseline.successor();

    match successor {
        None => {
            if baseline != max_path {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", successor);
                println!("candidate: {:?}", candidate);
                println!("\n\n\n");
                panic!("returned None when the path was NOT the greatest path! BoooOOOoo")
            }
        }
        Some(successor) => {
            if successor <= baseline {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", successor);
                println!("candidate: {:?}", candidate);
                println!("\n\n\n");

                panic!("successor was not greater than the path it was derived from! BooooOoooOOo")
            }

            if candidate < successor && candidate > baseline {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", successor);
                println!("candidate: {:?}", candidate);
                println!("\n\n\n");

                panic!("the successor generated was NOT the immediate successor! BooooOOOOo!")
            }
        }
    }
}

pub fn test_successor_of_prefix<const MCL: usize, const MCC: usize, const MPL: usize>(
    baseline: PathRc<MCL, MCC, MPL>,
    candidate: PathRc<MCL, MCC, MPL>,
    unsucceedable: &[PathRc<MCL, MCC, MPL>],
) {
    let prefix_successor = baseline.successor_of_prefix();

    match prefix_successor {
        None => {
            if !unsucceedable.iter().any(|unsuc| unsuc == &baseline) {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", prefix_successor);
                println!("candidate: {:?}", candidate);
                panic!("returned None when the path was NOT the greatest path! BoooOOOoo\n\n\n\n");
            }
        }
        Some(prefix_successor) => {
            if prefix_successor <= baseline {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", prefix_successor);
                println!("candidate: {:?}", candidate);
                panic!("the successor is meant to be greater than the baseline, but wasn't!! BOOOOOOOOO\n\n\n\n");
            }

            if prefix_successor.is_prefixed_by(&baseline) {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", prefix_successor);
                println!("candidate: {:?}", candidate);
                panic!("successor was prefixed by the path it was derived from! BoooOOooOOooOo\n\n\n\n");
            }

            if !baseline.is_prefix_of(&candidate)
                && candidate < prefix_successor
                && candidate > baseline
            {
                println!("\n\n\n");
                println!("baseline: {:?}", baseline);
                println!("successor: {:?}", prefix_successor);
                println!("candidate: {:?}", candidate);

                panic!(
                    "the successor generated was NOT the immediate prefix successor! BooooOOOOo!\n\n\n\n"
                );
            }
        }
    }
}
