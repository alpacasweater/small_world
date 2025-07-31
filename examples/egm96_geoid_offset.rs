use small_world::egm96::EGM96;
use std::path::Path;

fn main() {
    let path = Path::new("data/WW15MGH.DAC");

    match EGM96::new(path) {
        Ok(mut egm96) => {
            println!("EGM96 model loaded successfully!");
            println!("Geoid height at (0, 0): {:.2} meters", egm96.read_geoid_value(0, 0).unwrap());
            println!("Geoid offset at (7.9322759878,	-72.2528133628) = {}", egm96.offset(7.9322759878,	-72.2528133628));
        }
        Err(e) => {
            eprintln!("Failed to load EGM96 model: {}", e);
        }
    }
}
