use std::error::Error;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::egm96::EGM96;
use small_world::terrain::SrtmDataset;

const AGL_INPUT_M: f64 = 120.0;
const MSL_INPUT_M: f64 = 350.0;
const HAE_INPUT_M: f64 = 420.0;

fn query_gdal_bilinear(
    tile_path: &Path,
    lat_deg: f64,
    lon_deg: f64,
) -> Result<f64, Box<dyn Error>> {
    let output = Command::new("gdallocationinfo")
        .arg("-valonly")
        .arg("-r")
        .arg("bilinear")
        .arg("-wgs84")
        .arg(tile_path)
        .arg(format!("{lon_deg:.15}"))
        .arg(format!("{lat_deg:.15}"))
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "gdallocationinfo failed (status={}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()?)
}

fn query_proj_geoid_offsets(points: &[(f64, f64)]) -> Result<Vec<f64>, Box<dyn Error>> {
    let mut cmd = Command::new("cct");
    cmd.arg("-d")
        .arg("12")
        .arg("-I")
        .arg("+proj=vgridshift")
        .arg("+grids=us_nga_egm96_15.tif")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    {
        let stdin = child.stdin.as_mut().ok_or("failed to open cct stdin")?;
        for (lat_deg, lon_deg) in points {
            writeln!(stdin, "{lon_deg:.15} {lat_deg:.15} 0.0")?;
        }
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "cct failed (status={}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let mut out = Vec::with_capacity(points.len());
    for line in String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            return Err(format!("unexpected cct output line: `{line}`").into());
        }
        out.push(fields[2].parse::<f64>()?);
    }
    if out.len() != points.len() {
        return Err(format!(
            "cct returned {} rows for {} points",
            out.len(),
            points.len()
        )
        .into());
    }
    Ok(out)
}

fn main() -> Result<(), Box<dyn Error>> {
    if !Command::new("cct")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success()
    {
        return Err("cct (PROJ) is required on PATH".into());
    }
    if !Command::new("gdallocationinfo")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success()
    {
        return Err("gdallocationinfo (GDAL) is required on PATH".into());
    }

    let geoid_path = Path::new("data/WW15MGH.DAC");
    let hgt_path = Path::new("data/srtm/N39W077.hgt");
    if !geoid_path.exists() {
        return Err(format!("missing geoid dataset: {}", geoid_path.display()).into());
    }
    if !hgt_path.exists() {
        return Err(format!("missing hgt tile: {}", hgt_path.display()).into());
    }

    let geoid = EGM96::new(geoid_path)?;
    let terrain = SrtmDataset::new("data/srtm");
    let converter = AltitudeConverter::new(&geoid, &terrain);

    let points = [(39.15, -76.95), (39.55, -76.75), (39.85, -76.35)];
    let geoid_oracle = query_proj_geoid_offsets(&points)?;

    println!("# Real HGT Validation Against Trusted Oracles");
    println!("Datasets: terrain=data/srtm/N39W077.hgt, geoid=data/WW15MGH.DAC (EGM96).");
    println!(
        "Oracles: PROJ cct vgridshift (us_nga_egm96_15.tif) + GDAL gdallocationinfo bilinear."
    );
    println!("Inputs: AGL={AGL_INPUT_M:.1}m, MSL={MSL_INPUT_M:.1}m, HAE={HAE_INPUT_M:.1}m.\n");

    println!("| lat | lon | ours_ground_msl | oracle_ground_msl | Δground | ours_N | oracle_N | ΔN | ours_MSL_from_AGL | oracle_MSL_from_AGL | ΔMSL | ours_HAE_from_AGL | oracle_HAE_from_AGL | ΔHAE | ours_AGL_from_MSL | oracle_AGL_from_MSL | ΔAGL | ours_MSL_from_HAE | oracle_MSL_from_HAE | ΔMSL(HAE) |");
    println!("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");

    for ((lat_deg, lon_deg), geoid_oracle_m) in points.iter().zip(geoid_oracle.iter()) {
        let point = GeoPoint::new(*lat_deg, *lon_deg)?;
        let ground_ours = converter.ground_msl_m(*lat_deg, *lon_deg)?;
        let geoid_ours = converter.geoid_offset_m(*lat_deg, *lon_deg)?;
        let ground_oracle = query_gdal_bilinear(hgt_path, *lat_deg, *lon_deg)?;

        let msl_from_agl_ours = converter.convert_height_m(
            point,
            AGL_INPUT_M,
            VerticalFrame::Agl,
            VerticalFrame::Msl,
        )?;
        let hae_from_agl_ours = converter.convert_height_m(
            point,
            AGL_INPUT_M,
            VerticalFrame::Agl,
            VerticalFrame::Hae,
        )?;
        let agl_from_msl_ours = converter.convert_height_m(
            point,
            MSL_INPUT_M,
            VerticalFrame::Msl,
            VerticalFrame::Agl,
        )?;
        let msl_from_hae_ours = converter.convert_height_m(
            point,
            HAE_INPUT_M,
            VerticalFrame::Hae,
            VerticalFrame::Msl,
        )?;

        let msl_from_agl_oracle = ground_oracle + AGL_INPUT_M;
        let hae_from_agl_oracle = msl_from_agl_oracle + geoid_oracle_m;
        let agl_from_msl_oracle = MSL_INPUT_M - ground_oracle;
        let msl_from_hae_oracle = HAE_INPUT_M - geoid_oracle_m;

        println!(
            "| {lat_deg:.2} | {lon_deg:.2} | {ground_ours:.3} | {ground_oracle:.3} | {:+.4} | {geoid_ours:.4} | {geoid_oracle_m:.4} | {:+.4} | {msl_from_agl_ours:.3} | {msl_from_agl_oracle:.3} | {:+.4} | {hae_from_agl_ours:.3} | {hae_from_agl_oracle:.3} | {:+.4} | {agl_from_msl_ours:.3} | {agl_from_msl_oracle:.3} | {:+.4} | {msl_from_hae_ours:.3} | {msl_from_hae_oracle:.3} | {:+.4} |",
            ground_ours - ground_oracle,
            geoid_ours - geoid_oracle_m,
            msl_from_agl_ours - msl_from_agl_oracle,
            hae_from_agl_ours - hae_from_agl_oracle,
            agl_from_msl_ours - agl_from_msl_oracle,
            msl_from_hae_ours - msl_from_hae_oracle
        );
    }

    let mut sum_err = 0.0_f64;
    let mut sum_sq_err = 0.0_f64;
    let mut max_abs_err = 0.0_f64;
    let mut agl_sum_err = 0.0_f64;
    let mut agl_sum_sq_err = 0.0_f64;
    let mut agl_max_abs_err = 0.0_f64;

    let mut grid_points = Vec::new();
    for i in 0..8 {
        let lat = 39.05 + i as f64 * (0.90 / 7.0);
        for j in 0..8 {
            let lon = -76.95 + j as f64 * (0.90 / 7.0);
            grid_points.push((lat, lon));
        }
    }

    let count = grid_points.len() as f64;
    for (lat_deg, lon_deg) in grid_points {
        let point = GeoPoint::new(lat_deg, lon_deg)?;
        let ground_ours = converter.ground_msl_m(lat_deg, lon_deg)?;
        let ground_oracle = query_gdal_bilinear(hgt_path, lat_deg, lon_deg)?;
        let err = ground_ours - ground_oracle;
        sum_err += err;
        sum_sq_err += err * err;
        max_abs_err = max_abs_err.max(err.abs());

        let agl_ours = converter.convert_height_m(
            point,
            MSL_INPUT_M,
            VerticalFrame::Msl,
            VerticalFrame::Agl,
        )?;
        let agl_oracle = MSL_INPUT_M - ground_oracle;
        let agl_err = agl_ours - agl_oracle;
        agl_sum_err += agl_err;
        agl_sum_sq_err += agl_err * agl_err;
        agl_max_abs_err = agl_max_abs_err.max(agl_err.abs());
    }

    println!();
    println!("Ground MSL error stats (64 points, bilinear vs GDAL bilinear):");
    println!("  bias_m     = {:.6}", sum_err / count);
    println!("  rmse_m     = {:.6}", (sum_sq_err / count).sqrt());
    println!("  max_abs_m  = {:.6}", max_abs_err);
    println!(
        "AGL-from-MSL error stats (same points, MSL input {:.1} m):",
        MSL_INPUT_M
    );
    println!("  bias_m     = {:.6}", agl_sum_err / count);
    println!("  rmse_m     = {:.6}", (agl_sum_sq_err / count).sqrt());
    println!("  max_abs_m  = {:.6}", agl_max_abs_err);

    Ok(())
}
