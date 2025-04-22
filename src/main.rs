mod egm96;
use egm96::EGM96;
use std::path::Path;

fn main() {
    let path = Path::new("/Users/biggsba1/Sync copy/rust_playground/small_world/data/WW15MGH.DAC");

    match EGM96::new(path) {
        Ok(egm96) => {
            println!("EGM96 model loaded successfully!");
            println!("Geoid height at (0, 0): {:.2} meters", egm96.geoid[0][0]);
            egm96.offset(&7.9322759878,	&-72.2528133628);
        }
        Err(e) => {
            eprintln!("Failed to load EGM96 model: {}", e);
        }
    }
}
