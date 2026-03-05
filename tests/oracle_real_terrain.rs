use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::egm96::EGM96;
use small_world::terrain::SrtmDataset;

const TERRAIN_TILE_PATH: &str = "data/srtm/N39W077.hgt";
const GEOID_PATH: &str = "data/WW15MGH.DAC";
const GROUND_MSL_TOLERANCE_M: f64 = 0.05;
const GEOID_TOLERANCE_M: f64 = 0.05;
const HEIGHT_TOLERANCE_M: f64 = 0.05;

fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn require_oracles() -> bool {
    matches!(
        std::env::var("SMALL_WORLD_REQUIRE_EXTERNAL_ORACLES")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

fn skip_if_oracles_missing() -> bool {
    let cct_ok = command_exists("cct");
    let gdal_ok = command_exists("gdallocationinfo");
    if cct_ok && gdal_ok {
        return false;
    }
    if require_oracles() {
        panic!(
            "external oracles required but missing tools: cct={} gdallocationinfo={}",
            cct_ok, gdal_ok
        );
    }
    eprintln!(
        "skipping real-terrain oracle test: cct={} gdallocationinfo={}",
        cct_ok, gdal_ok
    );
    true
}

fn query_gdal_bilinear(tile_path: &Path, lat_deg: f64, lon_deg: f64) -> Result<f64, String> {
    let output = Command::new("gdallocationinfo")
        .arg("-valonly")
        .arg("-r")
        .arg("bilinear")
        .arg("-wgs84")
        .arg(tile_path)
        .arg(format!("{lon_deg:.15}"))
        .arg(format!("{lat_deg:.15}"))
        .output()
        .map_err(|err| format!("failed to execute gdallocationinfo: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "gdallocationinfo failed (status={}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let value = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .map_err(|err| format!("failed parsing gdallocationinfo output: {err}"))?;
    Ok(value)
}

fn query_proj_geoid_offsets(points: &[(f64, f64)]) -> Result<Vec<f64>, String> {
    let mut cmd = Command::new("cct");
    cmd.arg("-d")
        .arg("12")
        .arg("-I")
        .arg("+proj=vgridshift")
        .arg("+grids=us_nga_egm96_15.tif")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("failed to spawn cct: {err}"))?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "failed to open cct stdin".to_string())?;
        for (lat_deg, lon_deg) in points {
            writeln!(stdin, "{lon_deg:.15} {lat_deg:.15} 0.0")
                .map_err(|err| format!("failed writing cct input: {err}"))?;
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("failed waiting for cct: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "cct failed (status={}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mut offsets = Vec::with_capacity(points.len());
    for line in String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 3 {
            return Err(format!("unexpected cct output line: {line}"));
        }
        let offset = cols[2]
            .parse::<f64>()
            .map_err(|err| format!("failed parsing cct z column `{}`: {err}", cols[2]))?;
        offsets.push(offset);
    }

    if offsets.len() != points.len() {
        return Err(format!(
            "cct returned {} rows for {} points",
            offsets.len(),
            points.len()
        ));
    }
    Ok(offsets)
}

#[test]
fn real_terrain_oracle_alignment_is_within_tolerance() -> Result<(), String> {
    if skip_if_oracles_missing() {
        return Ok(());
    }

    let terrain_tile = Path::new(TERRAIN_TILE_PATH);
    if !terrain_tile.exists() {
        if require_oracles() {
            return Err(format!(
                "required real-terrain tile missing: {}",
                terrain_tile.display()
            ));
        }
        eprintln!(
            "skipping real-terrain oracle test because {} is missing",
            terrain_tile.display()
        );
        return Ok(());
    }

    let geoid_path = Path::new(GEOID_PATH);
    if !geoid_path.exists() {
        return Err(format!("missing geoid dataset: {}", geoid_path.display()));
    }

    let points = [
        (39.05, -76.95),
        (39.10, -76.90),
        (39.15, -76.85),
        (39.20, -76.80),
        (39.25, -76.75),
        (39.30, -76.70),
        (39.35, -76.65),
        (39.40, -76.60),
        (39.45, -76.55),
        (39.50, -76.50),
        (39.55, -76.45),
        (39.60, -76.40),
    ];

    let geoid = EGM96::new(geoid_path).map_err(|err| format!("failed opening geoid: {err}"))?;
    let terrain = SrtmDataset::new("data/srtm");
    let converter = AltitudeConverter::new(&geoid, &terrain);

    let geoid_oracle = query_proj_geoid_offsets(&points)?;

    let mut max_ground_err = 0.0_f64;
    let mut max_geoid_err = 0.0_f64;
    let mut max_height_err = 0.0_f64;

    let test_msl = 350.0;

    for (i, (lat_deg, lon_deg)) in points.iter().enumerate() {
        let point = GeoPoint::new(*lat_deg, *lon_deg).map_err(|err| err.to_string())?;

        let ground_ours = converter
            .ground_msl_m(*lat_deg, *lon_deg)
            .map_err(|err| err.to_string())?;
        let ground_gdal = query_gdal_bilinear(terrain_tile, *lat_deg, *lon_deg)?;
        max_ground_err = max_ground_err.max((ground_ours - ground_gdal).abs());

        let geoid_ours = converter
            .geoid_offset_m(*lat_deg, *lon_deg)
            .map_err(|err| err.to_string())?;
        max_geoid_err = max_geoid_err.max((geoid_ours - geoid_oracle[i]).abs());

        let hae_ours = converter
            .convert_height_m(point, test_msl, VerticalFrame::Msl, VerticalFrame::Hae)
            .map_err(|err| err.to_string())?;
        let hae_expected = test_msl + geoid_oracle[i];
        max_height_err = max_height_err.max((hae_ours - hae_expected).abs());
    }

    if max_ground_err > GROUND_MSL_TOLERANCE_M {
        return Err(format!(
            "ground_msl mismatch exceeds tolerance: max_err={max_ground_err:.6} m tol={GROUND_MSL_TOLERANCE_M:.6}"
        ));
    }
    if max_geoid_err > GEOID_TOLERANCE_M {
        return Err(format!(
            "geoid mismatch exceeds tolerance: max_err={max_geoid_err:.6} m tol={GEOID_TOLERANCE_M:.6}"
        ));
    }
    if max_height_err > HEIGHT_TOLERANCE_M {
        return Err(format!(
            "MSL->HAE mismatch exceeds tolerance: max_err={max_height_err:.6} m tol={HEIGHT_TOLERANCE_M:.6}"
        ));
    }

    eprintln!(
        "real terrain oracle max errors: ground={max_ground_err:.6}m geoid={max_geoid_err:.6}m msl_to_hae={max_height_err:.6}m"
    );

    Ok(())
}
