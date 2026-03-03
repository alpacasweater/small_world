use small_world::egm96::{EGM2008, EGM96};
use std::env;
use std::path::Path;
use std::time::Instant;

fn print_usage() {
    println!("Usage:");
    println!("  cargo run --example geoid_offset -- <egm96|egm2008> <dataset_path> <lat_deg> <lon_deg> [nearest|bilinear|bicubic]");
    println!("Example:");
    println!(
        "  cargo run --example geoid_offset -- egm96 data/WW15MGH.DAC -0.466744 0.0023 bicubic"
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 {
        print_usage();
        return;
    }

    let model = args[1].to_lowercase();
    let path = Path::new(&args[2]);
    let lat: f64 = args[3]
        .parse()
        .expect("lat_deg must be a valid floating-point value");
    let lon: f64 = args[4]
        .parse()
        .expect("lon_deg must be a valid floating-point value");
    let interpolation = args.get(5).map(String::as_str).unwrap_or("bilinear");

    let start = Instant::now();
    let result = match model.as_str() {
        "egm96" => {
            let geoid = EGM96::new(path).expect("failed to load EGM96 dataset");
            match interpolation {
                "nearest" => geoid.offset(lat, lon),
                "bilinear" => geoid.offset_bilinear(lat, lon),
                "bicubic" => geoid.offset_bicubic(lat, lon),
                _ => {
                    eprintln!("unknown interpolation mode: {interpolation}");
                    print_usage();
                    return;
                }
            }
        }
        "egm2008" => {
            let geoid = EGM2008::new(path).expect("failed to load EGM2008 dataset");
            match interpolation {
                "nearest" => geoid.offset(lat, lon),
                "bilinear" => geoid.offset_bilinear(lat, lon),
                "bicubic" => geoid.offset_bicubic(lat, lon),
                _ => {
                    eprintln!("unknown interpolation mode: {interpolation}");
                    print_usage();
                    return;
                }
            }
        }
        _ => {
            eprintln!("unknown model: {model}");
            print_usage();
            return;
        }
    };

    let duration = start.elapsed();
    match result {
        Ok(offset) => {
            println!("Model: {}", model);
            println!("Dataset: {}", path.display());
            println!("Input: lat={lat:.8} lon={lon:.8}");
            println!("Interpolation: {interpolation}");
            println!("Geoid offset (MSL->HAE): {offset:.4} meters");
            println!("Time: {:?}", duration);
        }
        Err(err) => {
            eprintln!("Failed to query geoid offset: {err}");
        }
    }
}
