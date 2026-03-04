use std::io::Write;
use std::process::{Command, Stdio};

use small_world::wgs84::{AltType, Lla, Ned};

const EARTH_MEAN_RADIUS_M: f64 = 6_371_008.8;
const ORIGIN_COUNT: usize = 36;
const POINTS_PER_ORIGIN: usize = 8;
const HORIZONTAL_TOLERANCE_M: f64 = 0.03;
const VERTICAL_TOLERANCE_M: f64 = 0.03;
const NED_COMPONENT_TOLERANCE_M: f64 = 0.04;

#[derive(Clone, Copy, Debug)]
struct OracleLla {
    lat_deg: f64,
    lon_deg: f64,
    hae_m: f64,
}

#[derive(Clone, Copy, Debug)]
struct OracleNed {
    n_m: f64,
    e_m: f64,
    d_m: f64,
}

#[derive(Clone, Copy, Debug)]
struct OracleEnu {
    e_m: f64,
    n_m: f64,
    u_m: f64,
}

fn proj_available() -> bool {
    Command::new("cct")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn require_proj() -> bool {
    matches!(
        std::env::var("SMALL_WORLD_REQUIRE_PROJ").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

fn skip_if_proj_unavailable() -> bool {
    if proj_available() {
        return false;
    }
    if require_proj() {
        panic!(
            "PROJ cct binary is required but missing. Install PROJ and ensure `cct` is on PATH."
        );
    }
    eprintln!("skipping PROJ oracle tests: `cct` not found on PATH");
    true
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

fn normalize_lon_deg(lon_deg: f64) -> f64 {
    let mut lon = (lon_deg + 180.0).rem_euclid(360.0) - 180.0;
    if lon == 180.0 {
        lon = -180.0;
    }
    lon
}

fn lat_lon_to_horizontal_error_m(a: OracleLla, b: OracleLla) -> f64 {
    let d_lat_rad = (a.lat_deg - b.lat_deg).to_radians();
    let d_lon_rad = (a.lon_deg - b.lon_deg).to_radians();
    let mean_lat_rad = ((a.lat_deg + b.lat_deg) * 0.5).to_radians();
    let north_m = d_lat_rad * EARTH_MEAN_RADIUS_M;
    let east_m = d_lon_rad * EARTH_MEAN_RADIUS_M * mean_lat_rad.cos();
    (north_m * north_m + east_m * east_m).sqrt()
}

fn cct_transform_rows(
    origin: OracleLla,
    input_rows: &[[f64; 3]],
    inverse: bool,
) -> Result<Vec<[f64; 3]>, String> {
    let mut cmd = Command::new("cct");
    cmd.arg("-d").arg("12");
    if inverse {
        cmd.arg("-I");
    }
    cmd.arg("+proj=pipeline")
        .arg("+step")
        .arg("+proj=cart")
        .arg("+ellps=WGS84")
        .arg("+step")
        .arg("+proj=topocentric")
        .arg("+ellps=WGS84")
        .arg(format!("+lon_0={:.15}", origin.lon_deg))
        .arg(format!("+lat_0={:.15}", origin.lat_deg))
        .arg(format!("+h_0={:.15}", origin.hae_m));
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("failed to launch cct: {err}"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "failed to open cct stdin".to_string())?;
        for row in input_rows {
            writeln!(stdin, "{:.15} {:.15} {:.15}", row[0], row[1], row[2])
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut parsed = Vec::with_capacity(input_rows.len());
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let values: Vec<&str> = line.split_whitespace().collect();
        if values.len() < 3 {
            return Err(format!("unexpected cct output line: `{line}`"));
        }
        let x = values[0]
            .parse::<f64>()
            .map_err(|err| format!("failed parsing `{}` from cct: {err}", values[0]))?;
        let y = values[1]
            .parse::<f64>()
            .map_err(|err| format!("failed parsing `{}` from cct: {err}", values[1]))?;
        let z = values[2]
            .parse::<f64>()
            .map_err(|err| format!("failed parsing `{}` from cct: {err}", values[2]))?;
        parsed.push([x, y, z]);
    }

    if parsed.len() != input_rows.len() {
        return Err(format!(
            "unexpected cct output row count: expected {}, got {}",
            input_rows.len(),
            parsed.len()
        ));
    }
    Ok(parsed)
}

fn proj_lla_to_ned(origin: OracleLla, points: &[OracleLla]) -> Result<Vec<OracleNed>, String> {
    let mut rows = Vec::with_capacity(points.len());
    for point in points {
        rows.push([point.lon_deg, point.lat_deg, point.hae_m]);
    }
    let enu_rows = cct_transform_rows(origin, &rows, false)?;
    let mut ned = Vec::with_capacity(enu_rows.len());
    for row in enu_rows {
        ned.push(OracleNed {
            n_m: row[1],
            e_m: row[0],
            d_m: -row[2],
        });
    }
    Ok(ned)
}

fn proj_ned_to_lla(origin: OracleLla, points: &[OracleNed]) -> Result<Vec<OracleLla>, String> {
    let mut rows = Vec::with_capacity(points.len());
    for point in points {
        rows.push([point.e_m, point.n_m, -point.d_m]);
    }
    let lla_rows = cct_transform_rows(origin, &rows, true)?;
    let mut lla = Vec::with_capacity(lla_rows.len());
    for row in lla_rows {
        lla.push(OracleLla {
            lon_deg: row[0],
            lat_deg: row[1],
            hae_m: row[2],
        });
    }
    Ok(lla)
}

fn sample_origin(seed: &mut u64) -> OracleLla {
    OracleLla {
        lat_deg: uniform(seed, -80.0, 80.0),
        lon_deg: uniform(seed, -179.9, 179.9),
        hae_m: uniform(seed, -500.0, 9000.0),
    }
}

fn sample_nearby_point(seed: &mut u64, origin: OracleLla) -> OracleLla {
    let lat = (origin.lat_deg + uniform(seed, -0.7, 0.7)).clamp(-89.9, 89.9);
    let lon = normalize_lon_deg(origin.lon_deg + uniform(seed, -0.7, 0.7));
    let hae = origin.hae_m + uniform(seed, -2500.0, 2500.0);
    OracleLla {
        lat_deg: lat,
        lon_deg: lon,
        hae_m: hae,
    }
}

fn sample_ned(seed: &mut u64) -> OracleNed {
    OracleNed {
        n_m: uniform(seed, -50_000.0, 50_000.0),
        e_m: uniform(seed, -50_000.0, 50_000.0),
        d_m: uniform(seed, -10_000.0, 10_000.0),
    }
}

#[test]
fn lla_to_ned_matches_proj_oracle() -> Result<(), String> {
    if skip_if_proj_unavailable() {
        return Ok(());
    }

    let mut seed = 0x4D59_5DF4_D0F3_3173;
    let mut max_component_err = 0.0_f64;

    for _ in 0..ORIGIN_COUNT {
        let origin = sample_origin(&mut seed);
        let origin_lla = Lla::new(origin.lat_deg, origin.lon_deg, origin.hae_m, AltType::Wgs84);

        let mut points = Vec::with_capacity(POINTS_PER_ORIGIN);
        for _ in 0..POINTS_PER_ORIGIN {
            points.push(sample_nearby_point(&mut seed, origin));
        }

        let oracle_ned = proj_lla_to_ned(origin, &points)?;
        for (point, oracle) in points.into_iter().zip(oracle_ned.into_iter()) {
            let our = Ned::from_lla(
                Lla::new(point.lat_deg, point.lon_deg, point.hae_m, AltType::Wgs84),
                origin_lla,
            );
            let component_err = (our.n() - oracle.n_m)
                .abs()
                .max((our.e() - oracle.e_m).abs())
                .max((our.d() - oracle.d_m).abs());
            max_component_err = max_component_err.max(component_err);

            if component_err > NED_COMPONENT_TOLERANCE_M {
                return Err(format!(
                    "NED mismatch above tolerance: err={component_err:.6} m \
                     (our n/e/d={:.6}/{:.6}/{:.6}, oracle n/e/d={:.6}/{:.6}/{:.6}) \
                     origin=({:.8},{:.8},{:.3}) point=({:.8},{:.8},{:.3})",
                    our.n(),
                    our.e(),
                    our.d(),
                    oracle.n_m,
                    oracle.e_m,
                    oracle.d_m,
                    origin.lat_deg,
                    origin.lon_deg,
                    origin.hae_m,
                    point.lat_deg,
                    point.lon_deg,
                    point.hae_m
                ));
            }
        }
    }

    eprintln!("max NED component error vs PROJ oracle: {max_component_err:.6} m");
    Ok(())
}

#[test]
fn ned_to_lla_matches_proj_oracle() -> Result<(), String> {
    if skip_if_proj_unavailable() {
        return Ok(());
    }

    let mut seed = 0x9E37_79B9_7F4A_7C15;
    let mut max_horizontal_m = 0.0_f64;
    let mut max_vertical_m = 0.0_f64;

    for _ in 0..ORIGIN_COUNT {
        let origin = sample_origin(&mut seed);
        let origin_lla = Lla::new(origin.lat_deg, origin.lon_deg, origin.hae_m, AltType::Wgs84);

        let mut ned_points = Vec::with_capacity(POINTS_PER_ORIGIN);
        for _ in 0..POINTS_PER_ORIGIN {
            ned_points.push(sample_ned(&mut seed));
        }

        let oracle_lla = proj_ned_to_lla(origin, &ned_points)?;
        for (ned, oracle) in ned_points.into_iter().zip(oracle_lla.into_iter()) {
            let our = Ned::new(ned.n_m, ned.e_m, ned.d_m, origin_lla).to_lla();
            let our_lla = OracleLla {
                lat_deg: our.lat_deg(),
                lon_deg: normalize_lon_deg(our.lon_deg()),
                hae_m: our.alt_m(),
            };
            let oracle_lla = OracleLla {
                lat_deg: oracle.lat_deg,
                lon_deg: normalize_lon_deg(oracle.lon_deg),
                hae_m: oracle.hae_m,
            };

            let horizontal_m = lat_lon_to_horizontal_error_m(our_lla, oracle_lla);
            let vertical_m = (our_lla.hae_m - oracle_lla.hae_m).abs();
            max_horizontal_m = max_horizontal_m.max(horizontal_m);
            max_vertical_m = max_vertical_m.max(vertical_m);

            if horizontal_m > HORIZONTAL_TOLERANCE_M || vertical_m > VERTICAL_TOLERANCE_M {
                return Err(format!(
                    "LLA mismatch above tolerance: horizontal={horizontal_m:.6} m vertical={vertical_m:.6} m \
                     (our lat/lon/hae={:.10}/{:.10}/{:.4}, oracle={:.10}/{:.10}/{:.4}) \
                     origin=({:.8},{:.8},{:.3}) ned=({:.3},{:.3},{:.3})",
                    our_lla.lat_deg,
                    our_lla.lon_deg,
                    our_lla.hae_m,
                    oracle_lla.lat_deg,
                    oracle_lla.lon_deg,
                    oracle_lla.hae_m,
                    origin.lat_deg,
                    origin.lon_deg,
                    origin.hae_m,
                    ned.n_m,
                    ned.e_m,
                    ned.d_m
                ));
            }
        }
    }

    eprintln!(
        "max LLA error vs PROJ oracle: horizontal={max_horizontal_m:.6} m vertical={max_vertical_m:.6} m"
    );
    Ok(())
}

#[test]
fn datum_edge_cases_match_proj_oracle() -> Result<(), String> {
    if skip_if_proj_unavailable() {
        return Ok(());
    }

    let origins = [
        OracleLla {
            lat_deg: 89.8,
            lon_deg: 45.0,
            hae_m: 10.0,
        },
        OracleLla {
            lat_deg: -89.8,
            lon_deg: -120.0,
            hae_m: 250.0,
        },
        OracleLla {
            lat_deg: 0.1,
            lon_deg: 179.8,
            hae_m: 100.0,
        },
        OracleLla {
            lat_deg: -0.1,
            lon_deg: -179.8,
            hae_m: -50.0,
        },
    ];

    let offsets = [
        OracleEnu {
            e_m: 0.0,
            n_m: 0.0,
            u_m: 0.0,
        },
        OracleEnu {
            e_m: 15_000.0,
            n_m: -8_000.0,
            u_m: 120.0,
        },
        OracleEnu {
            e_m: -22_000.0,
            n_m: 31_000.0,
            u_m: -80.0,
        },
    ];

    for origin in origins {
        let origin_lla = Lla::new(origin.lat_deg, origin.lon_deg, origin.hae_m, AltType::Wgs84);
        let ned_samples: Vec<OracleNed> = offsets
            .iter()
            .map(|offset| OracleNed {
                n_m: offset.n_m,
                e_m: offset.e_m,
                d_m: -offset.u_m,
            })
            .collect();
        let oracle_lla = proj_ned_to_lla(origin, &ned_samples)?;

        for (ned, oracle) in ned_samples.iter().zip(oracle_lla.iter()) {
            let our = Ned::new(ned.n_m, ned.e_m, ned.d_m, origin_lla).to_lla();
            let our_lla = OracleLla {
                lat_deg: our.lat_deg(),
                lon_deg: normalize_lon_deg(our.lon_deg()),
                hae_m: our.alt_m(),
            };
            let oracle_lla = OracleLla {
                lat_deg: oracle.lat_deg,
                lon_deg: normalize_lon_deg(oracle.lon_deg),
                hae_m: oracle.hae_m,
            };

            let horizontal_m = lat_lon_to_horizontal_error_m(our_lla, oracle_lla);
            let vertical_m = (our_lla.hae_m - oracle_lla.hae_m).abs();
            if horizontal_m > HORIZONTAL_TOLERANCE_M || vertical_m > VERTICAL_TOLERANCE_M {
                return Err(format!(
                    "edge-case mismatch: horizontal={horizontal_m:.6} m vertical={vertical_m:.6} m \
                     origin=({:.6},{:.6},{:.3}) ned=({:.3},{:.3},{:.3})",
                    origin.lat_deg,
                    origin.lon_deg,
                    origin.hae_m,
                    ned.n_m,
                    ned.e_m,
                    ned.d_m
                ));
            }
        }
    }

    Ok(())
}
