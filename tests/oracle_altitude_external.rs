use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::egm96::EGM96;
use small_world::terrain::SrtmDataset;

const ORACLE_SAMPLE_COUNT: usize = 96;
const HEIGHT_TOLERANCE_M: f64 = 0.05;
const LLA_ALT_TOLERANCE_M: f64 = 0.05;

fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| true)
        .unwrap_or(false)
}

fn require_external_oracles() -> bool {
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
    if require_external_oracles() {
        panic!(
            "external oracles required but missing tools: cct={} gdallocationinfo={}",
            cct_ok, gdal_ok
        );
    }
    eprintln!(
        "skipping external altitude oracle test: cct={} gdallocationinfo={}",
        cct_ok, gdal_ok
    );
    true
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "small_world_oracle_{label}_{}_{}",
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
            let value_m = 100_i16 + (2 * row + 3 * col) as i16;
            bytes.extend_from_slice(&value_m.to_be_bytes());
        }
    }
    fs::write(&tile_path, bytes).unwrap();
    tile_path
}

fn next_unit(seed: &mut u64) -> f64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 11) as f64) / ((1_u64 << 53) as f64)
}

fn uniform(seed: &mut u64, min: f64, max: f64) -> f64 {
    min + (max - min) * next_unit(seed)
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
    let text = String::from_utf8_lossy(&output.stdout);
    let value = text
        .trim()
        .parse::<f64>()
        .map_err(|err| format!("failed to parse gdallocationinfo output `{text}`: {err}"))?;
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
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            return Err(format!("unexpected cct line: `{line}`"));
        }
        let geoid_offset_m = fields[2]
            .parse::<f64>()
            .map_err(|err| format!("failed parsing cct z `{}`: {err}", fields[2]))?;
        offsets.push(geoid_offset_m);
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
fn altitude_conversions_match_proj_and_gdal_oracles() -> Result<(), String> {
    if skip_if_oracles_missing() {
        return Ok(());
    }

    let egm_path = Path::new("data/WW15MGH.DAC");
    if !egm_path.exists() {
        return Err(format!(
            "missing required geoid dataset: {}",
            egm_path.display()
        ));
    }

    let root = unique_temp_dir("hgt");
    let hgt_path = write_linear_hgt_tile(&root, 1201);
    let terrain = SrtmDataset::new(&root);
    let egm96 = EGM96::new(egm_path).map_err(|err| format!("failed opening egm96: {err}"))?;
    let converter = AltitudeConverter::new(&egm96, &terrain);

    let mut seed = 0xD1B5_4A32_D192_ED03;
    let mut points = Vec::with_capacity(ORACLE_SAMPLE_COUNT);
    for _ in 0..ORACLE_SAMPLE_COUNT {
        // Keep well inside the tile to avoid edge behavior divergence between implementations.
        let lat = uniform(&mut seed, 0.02, 0.98);
        let lon = uniform(&mut seed, 0.02, 0.98);
        points.push((lat, lon));
    }

    let geoid_offsets = query_proj_geoid_offsets(&points)?;

    let mut max_conversion_err = 0.0_f64;
    let mut max_lla_alt_err = 0.0_f64;

    let frames = [VerticalFrame::Agl, VerticalFrame::Msl, VerticalFrame::Hae];
    for (idx, (lat_deg, lon_deg)) in points.iter().enumerate() {
        let point = GeoPoint::new(*lat_deg, *lon_deg).map_err(|err| err.to_string())?;
        let ground_msl_m = query_gdal_bilinear(&hgt_path, *lat_deg, *lon_deg)?;
        let geoid_offset_m = geoid_offsets[idx];
        let input_m = uniform(&mut seed, -1000.0, 20_000.0);

        for source in frames {
            for target in frames {
                let expected_msl = match source {
                    VerticalFrame::Agl => ground_msl_m + input_m,
                    VerticalFrame::Msl => input_m,
                    VerticalFrame::Hae => input_m - geoid_offset_m,
                };
                let expected = match target {
                    VerticalFrame::Agl => expected_msl - ground_msl_m,
                    VerticalFrame::Msl => expected_msl,
                    VerticalFrame::Hae => expected_msl + geoid_offset_m,
                };

                let actual = converter
                    .convert_height_m(point, input_m, source, target)
                    .map_err(|err| err.to_string())?;
                let err = (actual - expected).abs();
                max_conversion_err = max_conversion_err.max(err);
                if err > HEIGHT_TOLERANCE_M {
                    return Err(format!(
                        "conversion mismatch above tolerance: err={err:.6} m \
                         source={source:?} target={target:?} lat={lat_deg:.8} lon={lon_deg:.8} \
                         input={input_m:.6} expected={expected:.6} actual={actual:.6} \
                         oracle_ground_msl={ground_msl_m:.6} oracle_geoid={geoid_offset_m:.6}"
                    ));
                }
            }

            let expected_hae = match source {
                VerticalFrame::Agl => ground_msl_m + input_m + geoid_offset_m,
                VerticalFrame::Msl => input_m + geoid_offset_m,
                VerticalFrame::Hae => input_m,
            };
            let lla = converter
                .lla_wgs84_from_height_m(point, input_m, source)
                .map_err(|err| err.to_string())?;
            let lla_err = (lla.alt_m() - expected_hae).abs();
            max_lla_alt_err = max_lla_alt_err.max(lla_err);
            if lla_err > LLA_ALT_TOLERANCE_M {
                return Err(format!(
                    "lla_wgs84_from_height_m mismatch above tolerance: err={lla_err:.6} m \
                     source={source:?} lat={lat_deg:.8} lon={lon_deg:.8} input={input_m:.6} \
                     expected_hae={expected_hae:.6} actual_hae={:.6}",
                    lla.alt_m()
                ));
            }
        }
    }

    eprintln!(
        "max AGL/MSL/HAE conversion error vs PROJ+GDAL oracles: {max_conversion_err:.6} m ({} points, {} frame pairs each)",
        ORACLE_SAMPLE_COUNT,
        frames.len() * frames.len()
    );
    eprintln!(
        "max lla_wgs84_from_height_m altitude error vs PROJ+GDAL oracles: {max_lla_alt_err:.6} m"
    );
    Ok(())
}
