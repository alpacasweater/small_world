use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::egm96::EGM96;
use small_world::terrain::SrtmDataset;

const AGL_INPUT_M: f64 = 120.0;
const MSL_INPUT_M: f64 = 350.0;
const HAE_INPUT_M: f64 = 420.0;

fn unique_temp_dir(label: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "small_world_oracle_example_{label}_{}_{}",
        std::process::id(),
        now
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_linear_hgt_tile(path: &Path, side: usize) -> PathBuf {
    let tile_path = path.join("N00E000.hgt");
    let mut bytes = Vec::with_capacity(side * side * 2);
    for row in 0..side {
        for col in 0..side {
            // Deterministic sloped surface to exercise bilinear interpolation.
            let value_m = 100_i16 + (2 * row + 3 * col) as i16;
            bytes.extend_from_slice(&value_m.to_be_bytes());
        }
    }
    fs::write(&tile_path, bytes).unwrap();
    tile_path
}

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

fn query_proj_geoid_offset(lat_deg: f64, lon_deg: f64) -> Result<f64, Box<dyn Error>> {
    let mut child = Command::new("cct")
        .arg("-d")
        .arg("12")
        .arg("-I")
        .arg("+proj=vgridshift")
        .arg("+grids=us_nga_egm96_15.tif")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().ok_or("failed to open cct stdin")?;
        writeln!(stdin, "{lon_deg:.15} {lat_deg:.15} 0.0")?;
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

    let line = String::from_utf8_lossy(&output.stdout);
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 3 {
        return Err(format!("unexpected cct output: `{line}`").into());
    }
    Ok(fields[2].parse::<f64>()?)
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

    let egm_path = Path::new("data/WW15MGH.DAC");
    if !egm_path.exists() {
        return Err("data/WW15MGH.DAC not found".into());
    }

    let root = unique_temp_dir("hgt");
    let hgt_path = write_linear_hgt_tile(&root, 1201);
    let terrain = SrtmDataset::new(&root);
    let egm96 = EGM96::new(egm_path)?;
    let converter = AltitudeConverter::new(&egm96, &terrain);

    let points = [(0.15, 0.20), (0.55, 0.40), (0.80, 0.75)];

    println!("# Altitude Cross-Check");
    println!(
        "Oracles: PROJ `cct` (EGM96 vgridshift `us_nga_egm96_15.tif`) + GDAL `gdallocationinfo` bilinear terrain sampling."
    );
    println!("Terrain dataset used here: generated SRTM-valid `N00E000.hgt` linear surface.");
    println!("Inputs: AGL={AGL_INPUT_M:.1} m, MSL={MSL_INPUT_M:.1} m, HAE={HAE_INPUT_M:.1} m.\n");

    println!("| lat | lon | ours_ground_msl | oracle_ground_msl | Δground | ours_geoid_N | oracle_geoid_N | ΔN | ours_MSL_from_AGL | oracle_MSL_from_AGL | ΔMSL | ours_HAE_from_AGL | oracle_HAE_from_AGL | ΔHAE | ours_AGL_from_MSL | oracle_AGL_from_MSL | ΔAGL | ours_MSL_from_HAE | oracle_MSL_from_HAE | ΔMSL(HAE) |");
    println!("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");

    for (lat_deg, lon_deg) in points {
        let point = GeoPoint::new(lat_deg, lon_deg)?;

        let ground_msl_ours = converter.ground_msl_m(lat_deg, lon_deg)?;
        let geoid_ours = converter.geoid_offset_m(lat_deg, lon_deg)?;

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

        let ground_msl_oracle = query_gdal_bilinear(&hgt_path, lat_deg, lon_deg)?;
        let geoid_oracle = query_proj_geoid_offset(lat_deg, lon_deg)?;
        let msl_from_agl_oracle = ground_msl_oracle + AGL_INPUT_M;
        let hae_from_agl_oracle = msl_from_agl_oracle + geoid_oracle;
        let agl_from_msl_oracle = MSL_INPUT_M - ground_msl_oracle;
        let msl_from_hae_oracle = HAE_INPUT_M - geoid_oracle;

        println!(
            "| {lat_deg:.2} | {lon_deg:.2} | {ground_msl_ours:.3} | {ground_msl_oracle:.3} | {:+.4} | {geoid_ours:.4} | {geoid_oracle:.4} | {:+.4} | {msl_from_agl_ours:.3} | {msl_from_agl_oracle:.3} | {:+.4} | {hae_from_agl_ours:.3} | {hae_from_agl_oracle:.3} | {:+.4} | {agl_from_msl_ours:.3} | {agl_from_msl_oracle:.3} | {:+.4} | {msl_from_hae_ours:.3} | {msl_from_hae_oracle:.3} | {:+.4} |",
            ground_msl_ours - ground_msl_oracle,
            geoid_ours - geoid_oracle,
            msl_from_agl_ours - msl_from_agl_oracle,
            hae_from_agl_ours - hae_from_agl_oracle,
            agl_from_msl_ours - agl_from_msl_oracle,
            msl_from_hae_ours - msl_from_hae_oracle
        );
    }

    Ok(())
}
