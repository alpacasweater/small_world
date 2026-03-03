#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]

use std::f64::consts::PI;

// WGS84 ellipsoid defined in meters
const SEMIMAJOR_AXIS: f64 = 6378137.0;
const SEMIMINOR_AXIS: f64 = 6356752.31424518;
const FIRST_ECCENTRICITY: f64 = 0.0818191908426215;
const FIRST_ECCENTRICITY_SQ: f64 = FIRST_ECCENTRICITY * FIRST_ECCENTRICITY;
const SECOND_ECCENTRICITY_SQ: f64 = FIRST_ECCENTRICITY_SQ / (1.0 - FIRST_ECCENTRICITY_SQ);

/// WGS84 geodetic point:
/// - latitude/longitude in decimal degrees
/// - ellipsoidal height (`HAE`) in meters above WGS84.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LlaWgs84 {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub hae_m: f64,
}

impl LlaWgs84 {
    pub const fn new(lat_deg: f64, lon_deg: f64, hae_m: f64) -> Self {
        Self {
            lat_deg,
            lon_deg,
            hae_m,
        }
    }
}

/// Local ENU point in meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnuMeters {
    pub east_m: f64,
    pub north_m: f64,
    pub up_m: f64,
}

impl EnuMeters {
    pub const fn new(east_m: f64, north_m: f64, up_m: f64) -> Self {
        Self {
            east_m,
            north_m,
            up_m,
        }
    }
}

/// Local NED point in meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NedMeters {
    pub north_m: f64,
    pub east_m: f64,
    pub down_m: f64,
}

impl NedMeters {
    pub const fn new(north_m: f64, east_m: f64, down_m: f64) -> Self {
        Self {
            north_m,
            east_m,
            down_m,
        }
    }
}

#[derive(Debug)]
struct GeoConversionParams {
    rot: [[f64; 3]; 3], // rotational matrix from ECEF to NED
    x0: f64,            // ECEF x coordinate of reference point in meters
    y0: f64,            // ECEF y coordinate of reference point in meters
    z0: f64,            // ECEF z coordinate of reference point in meters
}

impl GeoConversionParams {
    fn new(lat0_rad: f64, lon0_rad: f64, alt0_m: f64) -> Self {
        let mut rot: [[f64; 3]; 3] = [[0.0; 3]; 3];
        let lat0 = lat0_rad;
        let lon0 = lon0_rad;
        let alt0 = alt0_m;
        let nu0 = SEMIMAJOR_AXIS / (1.0 - FIRST_ECCENTRICITY_SQ * lat0.sin() * lat0.sin()).sqrt();
        let s_lat0 = lat0.sin();
        let c_lat0 = lat0.cos();
        let s_lon0 = lon0.sin();
        let c_lon0 = lon0.cos();
        rot[0][0] = -s_lat0 * c_lon0;
        rot[0][1] = -s_lat0 * s_lon0;
        rot[0][2] = c_lat0;
        rot[1][0] = -s_lon0;
        rot[1][1] = c_lon0;
        rot[1][2] = 0.0;
        rot[2][0] = -c_lat0 * c_lon0;
        rot[2][1] = -c_lat0 * s_lon0;
        rot[2][2] = -s_lat0;
        let x0 = (nu0 + alt0) * c_lat0 * c_lon0;
        let y0 = (nu0 + alt0) * c_lat0 * s_lon0;
        let z0 = (nu0 * (1.0 - FIRST_ECCENTRICITY_SQ) + alt0) * s_lat0;
        GeoConversionParams { rot, x0, y0, z0 }
    }
}

/// Converts an ENU point at one WGS84/HAE origin into an NED point at another WGS84/HAE origin.
pub fn enu_to_ned_between_origins(
    enu_point: EnuMeters,
    enu_origin: LlaWgs84,
    ned_origin: LlaWgs84,
) -> NedMeters {
    let enu_lat0_rad = enu_origin.lat_deg.to_radians();
    let enu_lon0_rad = enu_origin.lon_deg.to_radians();
    let ned_lat0_rad = ned_origin.lat_deg.to_radians();
    let ned_lon0_rad = ned_origin.lon_deg.to_radians();

    let mut x = 0.0;
    let mut y = 0.0;
    let mut z = 0.0;
    enu_to_ecef(
        &enu_point.east_m,
        &enu_point.north_m,
        &enu_point.up_m,
        &enu_lat0_rad,
        &enu_lon0_rad,
        &enu_origin.hae_m,
        &mut x,
        &mut y,
        &mut z,
    );

    let mut north_m = 0.0;
    let mut east_m = 0.0;
    let mut down_m = 0.0;
    ecef_to_ned(
        &x,
        &y,
        &z,
        &ned_lat0_rad,
        &ned_lon0_rad,
        &ned_origin.hae_m,
        &mut north_m,
        &mut east_m,
        &mut down_m,
    );

    NedMeters {
        north_m,
        east_m,
        down_m,
    }
}

/// Converts an NED point at a WGS84/HAE origin into absolute LLA (WGS84 + HAE).
pub fn ned_to_lla_wgs84(ned_point: NedMeters, ned_origin: LlaWgs84) -> LlaWgs84 {
    let lat0_rad = ned_origin.lat_deg.to_radians();
    let lon0_rad = ned_origin.lon_deg.to_radians();
    let mut lat_rad = 0.0;
    let mut lon_rad = 0.0;
    let mut hae_m = 0.0;
    ned_to_lla(
        &ned_point.north_m,
        &ned_point.east_m,
        &ned_point.down_m,
        &lat0_rad,
        &lon0_rad,
        &ned_origin.hae_m,
        &mut lat_rad,
        &mut lon_rad,
        &mut hae_m,
    );

    LlaWgs84 {
        lat_deg: lat_rad.to_degrees(),
        lon_deg: lon_rad.to_degrees(),
        hae_m,
    }
}

/// Converts an absolute LLA (WGS84 + HAE) into NED meters at a WGS84/HAE origin.
pub fn lla_to_ned_wgs84(point: LlaWgs84, ned_origin: LlaWgs84) -> NedMeters {
    let lat_rad = point.lat_deg.to_radians();
    let lon_rad = point.lon_deg.to_radians();
    let lat0_rad = ned_origin.lat_deg.to_radians();
    let lon0_rad = ned_origin.lon_deg.to_radians();
    let mut north_m = 0.0;
    let mut east_m = 0.0;
    let mut down_m = 0.0;
    lla_to_ned(
        &lat_rad,
        &lon_rad,
        &point.hae_m,
        &lat0_rad,
        &lon0_rad,
        &ned_origin.hae_m,
        &mut north_m,
        &mut east_m,
        &mut down_m,
    );

    NedMeters {
        north_m,
        east_m,
        down_m,
    }
}

//=======================================================================================
//==============================   Pure Generic Functions ===============================
//=======================================================================================
fn ned_to_ecef(
    n: &f64,
    e: &f64,
    d: &f64,
    lat0_rad: &f64,
    lon0_rad: &f64,
    alt0_m: &f64,
    x: &mut f64,
    y: &mut f64,
    z: &mut f64,
) {
    let cp = GeoConversionParams::new(*lat0_rad, *lon0_rad, *alt0_m);

    // Convert from NED to ECEF
    let dx = cp.rot[0][0] * n + cp.rot[1][0] * e + cp.rot[2][0] * d;
    let dy = cp.rot[0][1] * n + cp.rot[1][1] * e + cp.rot[2][1] * d;
    let dz = cp.rot[0][2] * n + cp.rot[1][2] * e + cp.rot[2][2] * d;
    *x = dx + cp.x0;
    *y = dy + cp.y0;
    *z = dz + cp.z0;
}

fn enu_to_ecef(
    e: &f64,
    n: &f64,
    u: &f64,
    lat0_rad: &f64,
    lon0_rad: &f64,
    alt0_m: &f64,
    x: &mut f64,
    y: &mut f64,
    z: &mut f64,
) {
    let down = -u;
    ned_to_ecef(n, e, &down, lat0_rad, lon0_rad, alt0_m, x, y, z);
}

fn ecef_to_ned(
    x: &f64,
    y: &f64,
    z: &f64,
    lat0_rad: &f64,
    lon0_rad: &f64,
    alt0_m: &f64,
    n: &mut f64,
    e: &mut f64,
    d: &mut f64,
) {
    let cp = GeoConversionParams::new(*lat0_rad, *lon0_rad, *alt0_m);

    // Find the difference between the point x, y, z to the reference point in ECEF
    let dx = x - cp.x0;
    let dy = y - cp.y0;
    let dz = z - cp.z0;

    *n = cp.rot[0][0] * dx + cp.rot[0][1] * dy + cp.rot[0][2] * dz;
    *e = cp.rot[1][0] * dx + cp.rot[1][1] * dy + cp.rot[1][2] * dz;
    *d = cp.rot[2][0] * dx + cp.rot[2][1] * dy + cp.rot[2][2] * dz;
}

fn ecef_to_enu(
    x: &f64,
    y: &f64,
    z: &f64,
    lat0_rad: &f64,
    lon0_rad: &f64,
    alt0_m: &f64,
    e: &mut f64,
    n: &mut f64,
    u: &mut f64,
) {
    ecef_to_ned(x, y, z, lat0_rad, lon0_rad, alt0_m, n, e, u);
    *u *= -1.;
}

fn ecef_to_lla(x: &f64, y: &f64, z: &f64, lat_rad: &mut f64, lon_rad: &mut f64, alt_m: &mut f64) {
    let p = (x * x + y * y).sqrt();
    let q = (z * SEMIMAJOR_AXIS).atan2(p * SEMIMINOR_AXIS);
    *lat_rad = (z + SECOND_ECCENTRICITY_SQ * SEMIMINOR_AXIS * q.sin().powi(3))
        .atan2(p - FIRST_ECCENTRICITY_SQ * SEMIMAJOR_AXIS * q.cos().powi(3));
    *lon_rad = y.atan2(*x);
    let nu = SEMIMAJOR_AXIS / (1.0 - FIRST_ECCENTRICITY_SQ * lat_rad.sin() * lat_rad.sin()).sqrt();
    *alt_m = p / lat_rad.cos() - nu;
}

fn lla_to_ecef(lat_rad: &f64, lon_rad: &f64, alt_m: &f64, x: &mut f64, y: &mut f64, z: &mut f64) {
    let s_lat = lat_rad.sin();
    let c_lat = lat_rad.cos();
    let s_lon = lon_rad.sin();
    let c_lon = lon_rad.cos();
    let nu = SEMIMAJOR_AXIS / (1.0 - FIRST_ECCENTRICITY_SQ * s_lat * s_lat).sqrt();

    *x = (nu + alt_m) * c_lat * c_lon;
    *y = (nu + alt_m) * c_lat * s_lon;
    *z = (nu * (1.0 - FIRST_ECCENTRICITY_SQ) + alt_m) * s_lat;
}

fn lla_to_ned(
    lat_rad: &f64,
    lon_rad: &f64,
    alt_m: &f64,
    lat0_rad: &f64,
    lon0_rad: &f64,
    alt0_m: &f64,
    n: &mut f64,
    e: &mut f64,
    d: &mut f64,
) {
    let mut x = 0.;
    let mut y = 0.;
    let mut z = 0.;
    lla_to_ecef(lat_rad, lon_rad, alt_m, &mut x, &mut y, &mut z);
    ecef_to_ned(&x, &y, &z, lat0_rad, lon0_rad, alt0_m, n, e, d);
}

fn lla_to_enu(
    lat_rad: &f64,
    lon_rad: &f64,
    alt_m: &f64,
    lat0_rad: &f64,
    lon0_rad: &f64,
    alt0_m: &f64,
    e: &mut f64,
    n: &mut f64,
    u: &mut f64,
) {
    let mut x = 0.;
    let mut y = 0.;
    let mut z = 0.;
    lla_to_ecef(lat_rad, lon_rad, alt_m, &mut x, &mut y, &mut z);
    ecef_to_enu(&x, &y, &z, lat0_rad, lon0_rad, alt0_m, e, n, u);
}

fn ned_to_lla(
    n: &f64,
    e: &f64,
    d: &f64,
    lat0_rad: &f64,
    lon0_rad: &f64,
    alt0_m: &f64,
    lat_rad: &mut f64,
    lon_rad: &mut f64,
    alt_m: &mut f64,
) {
    let mut x = 0.;
    let mut y = 0.;
    let mut z = 0.;
    ned_to_ecef(n, e, d, lat0_rad, lon0_rad, alt0_m, &mut x, &mut y, &mut z);
    ecef_to_lla(&x, &y, &z, lat_rad, lon_rad, alt_m);
}

fn enu_to_lla(
    e: &f64,
    n: &f64,
    u: &f64,
    lat0_rad: &f64,
    lon0_rad: &f64,
    alt0_m: &f64,
    lat_rad: &mut f64,
    lon_rad: &mut f64,
    alt_m: &mut f64,
) {
    let mut x = 0.;
    let mut y = 0.;
    let mut z = 0.;
    enu_to_ecef(e, n, u, lat0_rad, lon0_rad, alt0_m, &mut x, &mut y, &mut z);
    ecef_to_lla(&x, &y, &z, lat_rad, lon_rad, alt_m);
}

fn great_line_distance(
    lat0_rad: &f64,
    lon0_rad: &f64,
    alt0_m: &f64,
    lat1_rad: &f64,
    lon1_rad: &f64,
    alt1_m: &f64,
) -> f64 {
    let lon_diff = ((lon0_rad - lon1_rad) % PI).abs();
    let central_angle =
        (lat0_rad.sin() * lat1_rad.sin() + lat0_rad.cos() * lat1_rad.cos() * lon_diff.cos()).acos();
    let r = (2.0 * SEMIMAJOR_AXIS + SEMIMINOR_AXIS) / 3.0 + (alt0_m + alt1_m) / 2.0;

    r * central_angle
}

fn lla_euclidean_distance(
    lat0_rad: &f64,
    lon0_rad: &f64,
    alt0_m: &f64,
    lat1_rad: &f64,
    lon1_rad: &f64,
    alt1_m: &f64,
) -> f64 {
    let mut n1 = 0.0;
    let mut e1 = 0.0;
    let mut d1 = 0.0;
    lla_to_ned(
        lat1_rad, lon1_rad, alt1_m, lat0_rad, lon0_rad, alt0_m, &mut n1, &mut e1, &mut d1,
    );
    (n1 * n1 + e1 * e1 + d1 * d1).sqrt()
}

fn euclidean_distance(x0: &f64, y0: &f64, z0: &f64, x1: &f64, y1: &f64, z1: &f64) -> f64 {
    let x: f64 = x1 - x0;
    let y: f64 = y1 - y0;
    let z: f64 = z1 - z0;
    (x * x + y * y + z * z).sqrt()
}

fn heading_ned(n0: &f64, e0: &f64, n1: &f64, e1: &f64) -> f64 {
    let n = n1 - n0;
    let e = e1 - e0;
    e.atan2(n)
}

fn heading_enu(e0: &f64, n0: &f64, e1: &f64, n1: &f64) -> f64 {
    let e = e1 - e0;
    let n = n1 - n0;
    n.atan2(e)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{
        ecef_to_lla, ecef_to_ned, enu_to_lla, enu_to_ned_between_origins, euclidean_distance,
        lla_to_ecef, lla_to_ned_wgs84, ned_to_ecef, ned_to_lla_wgs84, EnuMeters, LlaWgs84,
        NedMeters,
    };

    #[test]
    fn lla_ecef_round_trip() {
        let lat = 38.628155_f64.to_radians();
        let lon = -76.8965265_f64.to_radians();
        let alt = 150.25_f64;

        let mut x = 0.0;
        let mut y = 0.0;
        let mut z = 0.0;
        lla_to_ecef(&lat, &lon, &alt, &mut x, &mut y, &mut z);

        let mut lat_out = 0.0;
        let mut lon_out = 0.0;
        let mut alt_out = 0.0;
        ecef_to_lla(&x, &y, &z, &mut lat_out, &mut lon_out, &mut alt_out);

        assert!((lat - lat_out).abs() < 1e-9);
        assert!((lon - lon_out).abs() < 1e-9);
        assert!((alt - alt_out).abs() < 1e-4);
    }

    #[test]
    fn ned_ecef_round_trip() {
        let lat0 = 39.1612306_f64.to_radians();
        let lon0 = -76.8965265_f64.to_radians();
        let alt0 = 33.0_f64;

        let n = 10.5_f64;
        let e = -4.25_f64;
        let d = 2.0_f64;

        let mut x = 0.0;
        let mut y = 0.0;
        let mut z = 0.0;
        ned_to_ecef(&n, &e, &d, &lat0, &lon0, &alt0, &mut x, &mut y, &mut z);

        let mut n_out = 0.0;
        let mut e_out = 0.0;
        let mut d_out = 0.0;
        ecef_to_ned(
            &x, &y, &z, &lat0, &lon0, &alt0, &mut n_out, &mut e_out, &mut d_out,
        );

        assert!((n - n_out).abs() < 1e-9);
        assert!((e - e_out).abs() < 1e-9);
        assert!((d - d_out).abs() < 1e-9);
    }

    #[test]
    fn euclidean_distance_matches_expected() {
        let d = euclidean_distance(&1.0, &2.0, &3.0, &4.0, &6.0, &3.0);
        assert!((d - 5.0).abs() < 1e-12);
    }

    #[test]
    fn enu_to_ned_same_origin_matches_axis_convention() {
        let origin = LlaWgs84::new(39.1612306, -76.8965265, 33.0);
        let enu = EnuMeters::new(12.0, -4.0, 7.5);
        let ned = enu_to_ned_between_origins(enu, origin, origin);

        assert!((ned.north_m + 4.0).abs() < 1e-9);
        assert!((ned.east_m - 12.0).abs() < 1e-9);
        assert!((ned.down_m + 7.5).abs() < 1e-9);
    }

    #[test]
    fn public_ned_lla_round_trip_is_consistent() {
        let origin = LlaWgs84::new(39.1612306, -76.8965265, 33.0);
        let ned = NedMeters::new(250.0, -120.0, 15.0);

        let point = ned_to_lla_wgs84(ned, origin);
        let ned_back = lla_to_ned_wgs84(point, origin);

        assert!((ned_back.north_m - ned.north_m).abs() < 1e-6);
        assert!((ned_back.east_m - ned.east_m).abs() < 1e-6);
        assert!((ned_back.down_m - ned.down_m).abs() < 1e-6);
    }

    proptest! {
        #[test]
        fn public_ned_lla_round_trip_randomized(
            lat_deg in -89.0f64..89.0,
            lon_deg in -180.0f64..180.0,
            hae_m in -200.0f64..12000.0,
            north_m in -10000.0f64..10000.0,
            east_m in -10000.0f64..10000.0,
            down_m in -5000.0f64..5000.0,
        ) {
            let origin = LlaWgs84::new(lat_deg, lon_deg, hae_m);
            let ned = NedMeters::new(north_m, east_m, down_m);

            let point = ned_to_lla_wgs84(ned, origin);
            let round_trip = lla_to_ned_wgs84(point, origin);

            prop_assert!((round_trip.north_m - north_m).abs() < 1e-4);
            prop_assert!((round_trip.east_m - east_m).abs() < 1e-4);
            prop_assert!((round_trip.down_m - down_m).abs() < 1e-4);
        }
    }

    proptest! {
        #[test]
        fn enu_to_ned_between_origins_matches_manual_chain(
            enu_origin_lat_deg in -85.0f64..85.0,
            enu_origin_lon_deg in -180.0f64..180.0,
            enu_origin_hae_m in -200.0f64..12000.0,
            ned_origin_lat_deg in -85.0f64..85.0,
            ned_origin_lon_deg in -180.0f64..180.0,
            ned_origin_hae_m in -200.0f64..12000.0,
            east_m in -5000.0f64..5000.0,
            north_m in -5000.0f64..5000.0,
            up_m in -2000.0f64..2000.0,
        ) {
            let enu_origin = LlaWgs84::new(enu_origin_lat_deg, enu_origin_lon_deg, enu_origin_hae_m);
            let ned_origin = LlaWgs84::new(ned_origin_lat_deg, ned_origin_lon_deg, ned_origin_hae_m);
            let enu_point = EnuMeters::new(east_m, north_m, up_m);

            let wrapper_output = enu_to_ned_between_origins(enu_point, enu_origin, ned_origin);

            let enu_origin_lat_rad = enu_origin.lat_deg.to_radians();
            let enu_origin_lon_rad = enu_origin.lon_deg.to_radians();

            let mut point_lat_rad = 0.0;
            let mut point_lon_rad = 0.0;
            let mut point_hae_m = 0.0;
            enu_to_lla(
                &enu_point.east_m,
                &enu_point.north_m,
                &enu_point.up_m,
                &enu_origin_lat_rad,
                &enu_origin_lon_rad,
                &enu_origin.hae_m,
                &mut point_lat_rad,
                &mut point_lon_rad,
                &mut point_hae_m,
            );

            let direct_point = LlaWgs84::new(
                point_lat_rad.to_degrees(),
                point_lon_rad.to_degrees(),
                point_hae_m,
            );
            let reconstructed_point = ned_to_lla_wgs84(wrapper_output, ned_origin);

            prop_assert!((reconstructed_point.lat_deg - direct_point.lat_deg).abs() < 1e-8);
            prop_assert!((reconstructed_point.lon_deg - direct_point.lon_deg).abs() < 1e-8);
            prop_assert!((reconstructed_point.hae_m - direct_point.hae_m).abs() < 1e-4);

            let round_trip_ned = lla_to_ned_wgs84(reconstructed_point, ned_origin);
            prop_assert!((round_trip_ned.north_m - wrapper_output.north_m).abs() < 1e-4);
            prop_assert!((round_trip_ned.east_m - wrapper_output.east_m).abs() < 1e-4);
            prop_assert!((round_trip_ned.down_m - wrapper_output.down_m).abs() < 1e-4);
        }
    }
}
