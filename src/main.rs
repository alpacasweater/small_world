extern crate nalgebra as na;
use na::{Matrix};
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
    lat0_: f64,         // Reference latitude in radians
    lon0_: f64,         // Reference longitude in radians
    alt0_: f64,         // Reference altitude in meters
    R_: [[f64; 3]; 3],  // rotational matrix from ECEF to NED
    x0_: f64,           // ECEF x coordinate of reference point
    y0_: f64,           // ECEF y coordinate of reference point
    z0_: f64,           // ECEF z coordinate of reference point
}

impl GeoConversionParams {
    fn new(lat0_rad: f64, lon0_rad: f64, alt0_m: f64) -> Self
    {
        let mut R_: [[f64; 3]; 3] = [[0.0; 3];3];
        let lat0_ = lat0_rad;
        let lon0_ = lon0_rad;   
        let alt0_ = alt0_m;  
        let nu0      = SEMIMAJOR_AXIS/(1.0 - FIRST_ECCENTRICITY_SQ*lat0_.sin()*lat0_.sin()).sqrt();
        let s_lat0_  = lat0_.sin();
        let c_lat0_  = lat0_.cos();
        let s_lon0_  = lon0_.sin();
        let c_lon0_  = lon0_.cos();
        R_[0][0]     = -s_lat0_*c_lon0_;
        R_[0][1]     = -s_lat0_*s_lon0_;
        R_[0][2]     =  c_lat0_;
        R_[1][0]     = -s_lon0_;
        R_[1][1]     =  c_lon0_;
        R_[1][2]     =  0.0;
        R_[2][0]     = -c_lat0_*c_lon0_;
        R_[2][1]     = -c_lat0_*s_lon0_;
        R_[2][2]     = -s_lat0_;
        let x0_      = (nu0 + alt0_)*c_lat0_*c_lon0_;
        let y0_      = (nu0 + alt0_)*c_lat0_*s_lon0_;
        let z0_      = (nu0*(1.0 - FIRST_ECCENTRICITY_SQ) + alt0_)*s_lat0_;
        GeoConversionParams
        {
            lat0_,
            lon0_,
            alt0_,
            R_,
            x0_,
            y0_,
            z0_,
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
    let dx = cp.R_[0][0]*n + cp.R_[1][0]*e + cp.R_[2][0]*d;
    let dy = cp.R_[0][1]*n + cp.R_[1][1]*e + cp.R_[2][1]*d;
    let dz = cp.R_[0][2]*n + cp.R_[1][2]*e + cp.R_[2][2]*d;
    *x      = dx + cp.x0_;
    *y      = dy + cp.y0_;
    *z      = dz + cp.z0_;
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
    let dx = x - cp.x0_;
    let dy = y - cp.y0_;
    let dz = z - cp.z0_;

    *n = cp.R_[0][0]*dx + cp.R_[0][1]*dy + cp.R_[0][2]*dz;
    *e = cp.R_[1][0]*dx + cp.R_[1][1]*dy + cp.R_[1][2]*dz;
    *d = cp.R_[2][0]*dx + cp.R_[2][1]*dy + cp.R_[2][2]*dz;
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

//=======================================================================================
//============================   nalgebra Based Functions ===============================
//=======================================================================================
// fn ned_to_ecef(ned: &Vector3d, lla0: &Vector3d, ecef: &mut Vector3d)
// {
//     ned_to_ecef(ned(0), ned(1), ned(2), lla0(0), lla0(1), lla0(2), ecef(0), ecef(1), ecef(2));
// }

// inline void enu_to_ecef(const Eigen::Vector3d& enu, const Eigen::Vector3d& lla0, Eigen::Vector3d& ecef)
// {
//     enu_to_ecef(enu(0), enu(1), enu(2), lla0(0), lla0(1), lla0(2), ecef(0), ecef(1), ecef(2));
// }

// inline void ecef_to_ned(const Eigen::Vector3d& ecef, const Eigen::Vector3d& lla0, Eigen::Vector3d& ned)
// {
//     ecef_to_ned(ecef(0), ecef(1), ecef(2), lla0(0), lla0(1), lla0(2), ned(0), ned(1), ned(2));
// }

// inline void ecef_to_enu(const Eigen::Vector3d& ecef, const Eigen::Vector3d& lla0, Eigen::Vector3d& enu)
// {
//     ecef_to_enu(ecef(0), ecef(1), ecef(2), lla0(0), lla0(1), lla0(2), enu(0), enu(1), enu(2));
// }

// inline void ecef_to_lla(const Eigen::Vector3d& ecef, Eigen::Vector3d& lla)
// {
//     ecef_to_lla(ecef(0), ecef(1), ecef(2), lla(0), lla(1), lla(2));
// }

// inline void lla_to_ecef(const Eigen::Vector3d& lla, Eigen::Vector3d& ecef)
// {
//     lla_to_ecef(lla(0), lla(1), lla(2), ecef(0), ecef(1), ecef(2));
// }

// inline void lla_to_ned(const Eigen::Vector3d& lla, const Eigen::Vector3d& lla0, Eigen::Vector3d& ned)
// {
//     lla_to_ned(lla(0), lla(1), lla(2), lla0(0), lla0(1), lla0(2), ned(0), ned(1), ned(2));
// }

// inline void lla_to_enu(const Eigen::Vector3d& lla, const Eigen::Vector3d& lla0, Eigen::Vector3d& enu)
// {
//     lla_to_enu(lla(0), lla(1), lla(2), lla0(0), lla0(1), lla0(2), enu(0), enu(1), enu(2));
// }

// inline void ned_to_lla(const Eigen::Vector3d& ned, const Eigen::Vector3d& lla0, Eigen::Vector3d& lla)
// {
//     ned_to_lla(ned(0), ned(1), ned(2), lla0(0), lla0(1), lla0(2), lla(0), lla(1), lla(2));
// }

// inline void enu_to_lla(const Eigen::Vector3d& enu, const Eigen::Vector3d& lla0, Eigen::Vector3d& lla)
// {
//     enu_to_lla(enu(0), enu(1), enu(2), lla0(0), lla0(1), lla0(2), lla(0), lla(1), lla(2));
// }

// inline double great_line_distance(const Eigen::Vector3d& lla0, const Eigen::Vector3d& lla1)
// {
//     return great_line_distance(lla0(0), lla0(1), lla0(2), lla1(0), lla1(1), lla1(2));
// }

// inline double lla_euclidean_distance(const Eigen::Vector3d& lla0, const Eigen::Vector3d& lla1)
// {
//     return lla_euclidean_distance(lla0(0), lla0(1), lla0(2), lla1(0), lla1(1), lla1(2));
// }

// inline double heading_ned(const Eigen::VectorXd& ned)
// {
//     return heading_ned(ned(0), ned(1));
// }

// inline double heading_enu(const Eigen::VectorXd& enu)
// {
//     return heading_enu(enu(0), enu(1));
// }

// inline double heading_ned(const Eigen::VectorXd& ned0, const Eigen::VectorXd& ned1)
// {
//     return heading_ned(ned0(0), ned0(1), ned1(0), ned1(1));
// }

// inline double heading_enu(const Eigen::VectorXd& enu0, const Eigen::VectorXd& enu1)
// {
//     return heading_enu(enu0(0), enu0(1), enu1(0), enu1(1));
// }

fn main() {
    let cp = GeoConversionParams::new(39.1612306f64.to_radians(),-76.8965265f64.to_radians(),33.0f64);
    println!("{:?}", cp);
}