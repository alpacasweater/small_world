use std::f64::consts::{PI};

// WGS84 ellipsoid defined in meters
const SEMIMAJOR_AXIS: f64 = 6378137.0;
const SEMIMINOR_AXIS: f64 = 6356752.31424518;
const INVERSE_FLATTENING: f64 = 298.257223563;
const FLATTENING: f64 = 1.0/INVERSE_FLATTENING;
const FIRST_ECCENTRICITY: f64 = 0.0818191908426215;
const FIRST_ECCENTRICITY_SQ: f64 = FIRST_ECCENTRICITY*FIRST_ECCENTRICITY;
const SECOND_ECCENTRICITY_SQ: f64 = FIRST_ECCENTRICITY_SQ / (1.0 -FIRST_ECCENTRICITY_SQ);

#[derive(Debug)]
struct GeoConversionParams {
    lat0: f64,         // Reference latitude in radians
    lon0: f64,         // Reference longitude in radians
    alt0: f64,         // Reference altitude in meters (WGS84)
    rot: [[f64; 3]; 3],  // rotational matrix from ECEF to NED
    x0: f64,           // ECEF x coordinate of reference point in meters
    y0: f64,           // ECEF y coordinate of reference point in meters
    z0: f64,           // ECEF z coordinate of reference point in meters
}

impl GeoConversionParams {
    fn new(lat0_rad: f64, lon0_rad: f64, alt0_m: f64) -> Self
    {
        let mut rot: [[f64; 3]; 3] = [[0.0; 3];3];
        let lat0 = lat0_rad;
        let lon0 = lon0_rad;   
        let alt0 = alt0_m;  
        let nu0      = SEMIMAJOR_AXIS/(1.0 - FIRST_ECCENTRICITY_SQ*lat0.sin()*lat0.sin()).sqrt();
        let s_lat0  = lat0.sin();
        let c_lat0  = lat0.cos();
        let s_lon0  = lon0_.sin();
        let c_lon0  = lon0_.cos();
        rot[0][0]     = -s_lat0*c_lon0_;
        rot[0][1]     = -s_lat0*s_lon0_;
        rot[0][2]     =  c_lat0;
        rot[1][0]     = -s_lon0_;
        rot[1][1]     =  c_lon0_;
        rot[1][2]     =  0.0;
        rot[2][0]     = -c_lat0*c_lon0_;
        rot[2][1]     = -c_lat0*s_lon0_;
        rot[2][2]     = -s_lat0;
        let x0      = (nu0 + alt0_)*c_lat0*c_lon0_;
        let y0      = (nu0 + alt0_)*c_lat0*s_lon0_;
        let z0      = (nu0*(1.0 - FIRST_ECCENTRICITY_SQ) + alt0_)*s_lat0;
        GeoConversionParams
        {
            lat0,
            lon0,
            alt0,
            rot,
            x0,
            y0,
            z0,
        }
    }
}

//=======================================================================================
//==============================   Pure Generic Functions ===============================
//=======================================================================================
fn ned_to_ecef( n: &f64, e: &f64, d: &f64,
                lat0_rad: &f64, lon0_rad: &f64, alt0_m: &f64,
                x: &mut f64, y: &mut f64, z: &mut f64)
{
    let cp = GeoConversionParams::new(*lat0_rad, *lon0_rad, *alt0_m);

    // Convert from NED to ECEF
    let dx = cp.rot[0][0]*n + cp.rot[1][0]*e + cp.rot[2][0]*d;
    let dy = cp.rot[0][1]*n + cp.rot[1][1]*e + cp.rot[2][1]*d;
    let dz = cp.rot[0][2]*n + cp.rot[1][2]*e + cp.rot[2][2]*d;
    *x      = dx + cp.x0;
    *y      = dy + cp.y0;
    *z      = dz + cp.z0;
}

fn enu_to_ecef( e: &f64, n: &f64, u: &f64,
                lat0_rad: &f64, lon0_rad: &f64, alt0_m: &f64,
                x: &mut f64, y: &mut f64, z: &mut f64)
{
    let down = -u;
    ned_to_ecef(n, e, &down, lat0_rad, lon0_rad, alt0_m, x, y, z);
}

fn ecef_to_ned( x: &f64, y: &f64, z: &f64,
                lat0_rad: &f64, lon0_rad: &f64, alt0_m: &f64,
                n: &mut f64, e: &mut f64, d: &mut f64)
{
    let cp = GeoConversionParams::new(*lat0_rad, *lon0_rad, *alt0_m);

    // Find the difference between the point x, y, z to the reference point in ECEF
    let dx = x - cp.x0;
    let dy = y - cp.y0;
    let dz = z - cp.z0;

    *n = cp.rot[0][0]*dx + cp.rot[0][1]*dy + cp.rot[0][2]*dz;
    *e = cp.rot[1][0]*dx + cp.rot[1][1]*dy + cp.rot[1][2]*dz;
    *d = cp.rot[2][0]*dx + cp.rot[2][1]*dy + cp.rot[2][2]*dz;
}

fn ecef_to_enu( x: &f64, y: &f64, z: &f64,
                lat0_rad: &f64, lon0_rad: &f64, alt0_m: &f64,
                e: &mut f64, n: &mut f64, u: &mut f64)
{
    ecef_to_ned(x, y, z, lat0_rad, lon0_rad, alt0_m, n, e, u);
    *u *= -1.;
}

fn ecef_to_lla( x: &f64, y: &f64, z: &f64,
                lat_rad: &mut f64, lon_rad: &mut f64, alt_m: &mut f64)
{
    let p       = (x*x + y*y).sqrt();
    let q       = (z*SEMIMAJOR_AXIS).atan2(p*SEMIMINOR_AXIS);
    *lat_rad    = (z + SECOND_ECCENTRICITY_SQ*SEMIMINOR_AXIS*q.sin().powi(3)).atan2(p - FIRST_ECCENTRICITY_SQ*SEMIMAJOR_AXIS*q.cos().powi(3));
    *lon_rad    = y.atan2(*x);
    let nu      = SEMIMAJOR_AXIS/(1.0 - FIRST_ECCENTRICITY_SQ*lat_rad.sin()*lat_rad.sin()).sqrt();
    *alt_m      = p/lat_rad.cos() - nu;
}

fn lla_to_ecef( lat_rad: &f64, lon_rad: &f64, alt_m: &f64,
                x: &mut f64, y: &mut f64, z: &mut f64)
{
    let s_lat = lat_rad.sin();
    let c_lat = lat_rad.cos();
    let s_lon = lon_rad.sin();
    let c_lon = lon_rad.cos();
    let nu    = SEMIMAJOR_AXIS/(1.0 - FIRST_ECCENTRICITY_SQ*s_lat*s_lat).sqrt();
    
    *x = (nu + alt_m)*c_lat*c_lon;
    *y = (nu + alt_m)*c_lat*s_lon;
    *z = (nu*(1.0 - FIRST_ECCENTRICITY_SQ) + alt_m)*s_lat;
}

fn lla_to_ned(  lat_rad: &f64, lon_rad: &f64, alt_m: &f64,
                lat0_rad: &f64, lon0_rad: &f64, alt0_m: &f64,
                n: &mut f64, e: &mut f64, d: &mut f64)
{
    let mut x = 0.;
    let mut y = 0.;
    let mut z = 0.;
    lla_to_ecef( lat_rad, lon_rad, alt_m, &mut x, &mut y, &mut z);
    ecef_to_ned( &x, &y, &z, lat0_rad, lon0_rad, alt0_m, n, e, d);
}

fn lla_to_enu(  lat_rad: &f64, lon_rad: &f64, alt_m: &f64,
                lat0_rad: &f64, lon0_rad: &f64, alt0_m: &f64,
                e: &mut f64, n: &mut f64, u: &mut f64)
{
    let mut x = 0.;
    let mut y = 0.;
    let mut z = 0.;
    lla_to_ecef( lat_rad, lon_rad, alt_m, &mut x, &mut y, &mut z);
    ecef_to_enu( &x, &y, &z, lat0_rad, lon0_rad, alt0_m, e, n, u);
}

fn ned_to_lla(  n: &f64, e: &f64, d: &f64,
                lat0_rad: &f64, lon0_rad: &f64, alt0_m: &f64,
                lat_rad: &mut f64, lon_rad: &mut f64, alt_m: &mut f64)
{
    let mut x = 0.;
    let mut y = 0.;
    let mut z = 0.;
    ned_to_ecef( n, e, d, lat0_rad, lon0_rad, alt0_m, &mut x, &mut y, &mut z);
    ecef_to_lla( &x, &y, &z, lat_rad, lon_rad, alt_m);
}

fn enu_to_lla(  e: &f64, n: &f64, u: &f64,
                lat0_rad: &f64, lon0_rad: &f64, alt0_m: &f64,
                lat_rad: &mut f64, lon_rad: &mut f64, alt_m: &mut f64)
{
    let mut x = 0.;
    let mut y = 0.;
    let mut z = 0.;
    enu_to_ecef( e, n, u, lat0_rad, lon0_rad, alt0_m, &mut x, &mut y, &mut z);
    ecef_to_lla( &x, &y, &z, lat_rad, lon_rad, alt_m);
}

fn great_line_distance( lat0_rad: &f64, lon0_rad: &f64, alt0_m: &f64,
                        lat1_rad: &f64, lon1_rad: &f64, alt1_m: &f64) -> f64
{
    let lon_diff = ((lon0_rad - lon1_rad)%PI).abs();
    let central_angle = (lat0_rad.sin()*lat1_rad.sin() + lat0_rad.cos()*lat1_rad.cos()*lon_diff.cos()).acos();
    let r = (2.0*SEMIMAJOR_AXIS + SEMIMINOR_AXIS)/3.0 + (alt0_m + alt1_m)/2.0;
    
    return r*central_angle
}

fn lla_euclidean_distance(  lat0_rad: &f64, lon0_rad: &f64, alt0_m: &f64,
                            lat1_rad: &f64, lon1_rad: &f64, alt1_m: &f64) -> f64
{
    let mut n1 = 0.0;
    let mut e1 = 0.0;
    let mut d1 = 0.0;
    lla_to_ned(lat1_rad, lon1_rad, alt1_m, lat0_rad, lon0_rad, alt0_m, &mut n1, &mut e1, &mut d1);
    return (n1*n1 + e1*e1 + d1*d1).sqrt();
}

fn euclidean_distance(  x0: &f64, y0: &f64, z0: &f64,
                        x1: &f64, y1: &f64, z1: &f64) -> f64
{
    let x: f64 = 0.0;
    let y: f64 = 0.0;
    let z: f64 = 0.0;
    return (x*x + y*y + z*z).sqrt();
}

fn heading_ned( n0: &f64, e0: &f64,
                n1: &f64, e1: &f64) -> f64
{
    let n = n1 - n0;
    let e = e1 - e0;
    return e.atan2(n)
}

fn heading_enu( e0: &f64, n0: &f64,
                e1: &f64, n1: &f64) -> f64
{
    let e = e1 - e0;
    let n = n1 - n0;
    return n.atan2(e)
}

fn main() {
    let cp = GeoConversionParams::new(39.1612306f64.to_radians(),-76.8965265f64.to_radians(),33.0f64);
    println!("{:?}", cp);
}