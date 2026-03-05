use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::egm96::EGM96;
use small_world::height::Interpolation;
use small_world::terrain::SrtmDataset;

const GEOID_PATH: &str = "data/WW15MGH.DAC";
const GROUND_MSL_TOLERANCE_M: f64 = 0.05;
const GEOID_TOLERANCE_M: f64 = 0.05;
const HEIGHT_TOLERANCE_M: f64 = 0.05;

#[derive(Clone, Copy)]
struct RegionCase {
    tile: &'static str,
    points: &'static [(f64, f64)],
}

const REGION_CASES: &[RegionCase] = &[
    RegionCase {
        tile: "N39W077.hgt",
        points: &[
            (39.05, -76.95),
            (39.20, -76.80),
            (39.40, -76.60),
            (39.55, -76.45),
        ],
    },
    RegionCase {
        tile: "N35E139.hgt",
        points: &[
            (35.10, 139.10),
            (35.25, 139.25),
            (35.45, 139.45),
            (35.70, 139.70),
        ],
    },
    RegionCase {
        tile: "N37E127.hgt",
        points: &[
            (37.10, 127.10),
            (37.30, 127.30),
            (37.50, 127.50),
            (37.75, 127.75),
        ],
    },
    RegionCase {
        tile: "S33E151.hgt",
        points: &[
            (-32.95, 151.05),
            (-32.80, 151.20),
            (-32.60, 151.40),
            (-32.35, 151.65),
        ],
    },
    RegionCase {
        tile: "S22W043.hgt",
        points: &[
            (-21.95, -42.95),
            (-21.80, -42.80),
            (-21.60, -42.60),
            (-21.35, -42.35),
        ],
    },
    RegionCase {
        tile: "N51E000.hgt",
        points: &[(51.05, 0.05), (51.20, 0.20), (51.40, 0.40), (51.70, 0.70)],
    },
    RegionCase {
        tile: "N27E086.hgt",
        points: &[
            (27.05, 86.05),
            (27.20, 86.20),
            (27.40, 86.40),
            (27.70, 86.70),
        ],
    },
];

fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| true)
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

fn gdal_supports_resampling_flag() -> bool {
    let output = match Command::new("gdallocationinfo").arg("--help").output() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text.contains("[-r") || text.contains(" -r ")
}

fn query_gdal_height(
    tile_path: &Path,
    lat_deg: f64,
    lon_deg: f64,
    use_bilinear: bool,
) -> Result<f64, String> {
    let mut cmd = Command::new("gdallocationinfo");
    cmd.arg("-valonly");
    if use_bilinear {
        cmd.arg("-r").arg("bilinear");
    }
    cmd.arg("-wgs84")
        .arg(tile_path)
        .arg(format!("{lon_deg:.15}"))
        .arg(format!("{lat_deg:.15}"));
    let output = cmd
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

fn gather_cases() -> Result<Vec<(PathBuf, f64, f64)>, String> {
    let mut gathered = Vec::new();
    for case in REGION_CASES {
        let tile_path = Path::new("data/srtm").join(case.tile);
        if !tile_path.exists() {
            if require_oracles() {
                return Err(format!(
                    "required real-terrain tile missing: {}",
                    tile_path.display()
                ));
            }
            eprintln!(
                "skipping real-terrain oracle test because {} is missing",
                tile_path.display()
            );
            return Ok(Vec::new());
        }
        for (lat, lon) in case.points {
            gathered.push((tile_path.clone(), *lat, *lon));
        }
    }
    Ok(gathered)
}

#[test]
fn real_terrain_oracle_alignment_is_within_tolerance() -> Result<(), String> {
    if skip_if_oracles_missing() {
        return Ok(());
    }

    let geoid_path = Path::new(GEOID_PATH);
    if !geoid_path.exists() {
        return Err(format!("missing geoid dataset: {}", geoid_path.display()));
    }

    let cases = gather_cases()?;
    if cases.is_empty() {
        return Ok(());
    }

    let all_points: Vec<(f64, f64)> = cases.iter().map(|(_, lat, lon)| (*lat, *lon)).collect();
    let geoid_oracle = query_proj_geoid_offsets(&all_points)?;
    let use_bilinear = gdal_supports_resampling_flag();
    let terrain_interp = if use_bilinear {
        Interpolation::Bilinear
    } else {
        Interpolation::Nearest
    };

    let geoid = EGM96::new(geoid_path).map_err(|err| format!("failed opening geoid: {err}"))?;
    let terrain = SrtmDataset::new("data/srtm");
    let converter =
        AltitudeConverter::new(&geoid, &terrain).with_terrain_interpolation(terrain_interp);

    let mut max_ground_err = 0.0_f64;
    let mut max_geoid_err = 0.0_f64;
    let mut max_height_err = 0.0_f64;
    let test_msl = 350.0;

    for (i, (tile_path, lat_deg, lon_deg)) in cases.iter().enumerate() {
        let point = GeoPoint::new(*lat_deg, *lon_deg).map_err(|err| err.to_string())?;

        let ground_ours = converter
            .ground_msl_m(*lat_deg, *lon_deg)
            .map_err(|err| err.to_string())?;
        let ground_gdal = query_gdal_height(tile_path, *lat_deg, *lon_deg, use_bilinear)?;
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
        "real terrain oracle max errors across {} regions / {} points: ground={max_ground_err:.6}m geoid={max_geoid_err:.6}m msl_to_hae={max_height_err:.6}m",
        REGION_CASES.len(),
        cases.len()
    );

    Ok(())
}
