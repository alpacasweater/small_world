use std::env;
use std::fs;
use std::path::Path;

use small_world::height::Interpolation;
use small_world::terrain::SrtmDataset;

#[derive(Clone, Copy)]
struct Checkpoint {
    lat_deg: f64,
    lon_deg: f64,
    truth_ground_msl_m: f64,
}

fn print_usage() {
    println!("Usage:");
    println!("  cargo run --example ground_msl_accuracy -- <srtm_dir> <checkpoints_csv> [nearest|bilinear|bicubic] [max_rmse_m] [max_abs_m]");
    println!("CSV format:");
    println!("  lat_deg,lon_deg,ground_msl_m");
}

fn parse_interpolation(value: Option<&str>) -> Option<Interpolation> {
    match value.unwrap_or("bilinear").to_ascii_lowercase().as_str() {
        "nearest" => Some(Interpolation::Nearest),
        "bilinear" => Some(Interpolation::Bilinear),
        "bicubic" => Some(Interpolation::Bicubic),
        _ => None,
    }
}

fn parse_checkpoints(path: &Path) -> Result<Vec<Checkpoint>, String> {
    let content = fs::read_to_string(path)
        .map_err(|err| format!("failed to read checkpoint CSV {}: {err}", path.display()))?;
    let mut checkpoints = Vec::new();

    for (line_index, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        if fields.len() != 3 {
            return Err(format!(
                "line {}: expected 3 comma-separated values, got {}",
                line_index + 1,
                fields.len()
            ));
        }

        let lat_deg = match fields[0].parse::<f64>() {
            Ok(value) => value,
            Err(_) if line_index == 0 => {
                // Allow header row.
                continue;
            }
            Err(err) => {
                return Err(format!(
                    "line {}: invalid lat_deg '{}': {err}",
                    line_index + 1,
                    fields[0]
                ))
            }
        };
        let lon_deg = fields[1].parse::<f64>().map_err(|err| {
            format!(
                "line {}: invalid lon_deg '{}': {err}",
                line_index + 1,
                fields[1]
            )
        })?;
        let truth_ground_msl_m = fields[2].parse::<f64>().map_err(|err| {
            format!(
                "line {}: invalid ground_msl_m '{}': {err}",
                line_index + 1,
                fields[2]
            )
        })?;
        checkpoints.push(Checkpoint {
            lat_deg,
            lon_deg,
            truth_ground_msl_m,
        });
    }

    if checkpoints.is_empty() {
        return Err("no checkpoint rows found".to_string());
    }

    Ok(checkpoints)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        print_usage();
        std::process::exit(2);
    }

    let srtm_dir = Path::new(&args[1]);
    let checkpoints_csv = Path::new(&args[2]);
    let interpolation = parse_interpolation(args.get(3).map(String::as_str))
        .expect("invalid interpolation (nearest|bilinear|bicubic)");
    let max_rmse_m = args.get(4).and_then(|value| value.parse::<f64>().ok());
    let max_abs_m = args.get(5).and_then(|value| value.parse::<f64>().ok());

    let checkpoints = parse_checkpoints(checkpoints_csv).expect("invalid checkpoint CSV");
    let terrain = SrtmDataset::new(srtm_dir);

    let mut sum_error = 0.0_f64;
    let mut sum_sq_error = 0.0_f64;
    let mut max_abs_error = 0.0_f64;
    let mut num_failed_queries = 0usize;

    for checkpoint in &checkpoints {
        match terrain.elevation_msl_with_interpolation(
            checkpoint.lat_deg,
            checkpoint.lon_deg,
            interpolation,
        ) {
            Ok(predicted_ground_msl_m) => {
                let error = predicted_ground_msl_m - checkpoint.truth_ground_msl_m;
                sum_error += error;
                sum_sq_error += error * error;
                max_abs_error = max_abs_error.max(error.abs());
            }
            Err(err) => {
                num_failed_queries += 1;
                eprintln!(
                    "query failed at lat={}, lon={}: {}",
                    checkpoint.lat_deg, checkpoint.lon_deg, err
                );
            }
        }
    }

    if num_failed_queries > 0 {
        eprintln!(
            "failed {} of {} checkpoint queries",
            num_failed_queries,
            checkpoints.len()
        );
        std::process::exit(1);
    }

    let count = checkpoints.len() as f64;
    let bias_m = sum_error / count;
    let rmse_m = (sum_sq_error / count).sqrt();

    println!("Checkpoint count: {}", checkpoints.len());
    println!("Bias (m): {:.6}", bias_m);
    println!("RMSE (m): {:.6}", rmse_m);
    println!("Max abs error (m): {:.6}", max_abs_error);

    if let Some(max_rmse_m) = max_rmse_m {
        if rmse_m > max_rmse_m {
            eprintln!(
                "RMSE threshold failed: measured {:.6} m > allowed {:.6} m",
                rmse_m, max_rmse_m
            );
            std::process::exit(1);
        }
    }
    if let Some(max_abs_m) = max_abs_m {
        if max_abs_error > max_abs_m {
            eprintln!(
                "Max-abs threshold failed: measured {:.6} m > allowed {:.6} m",
                max_abs_error, max_abs_m
            );
            std::process::exit(1);
        }
    }
}
