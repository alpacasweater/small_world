use std::error::Error;
use std::path::Path;

use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::egm96::{EGM2008, EGM96};
use small_world::terrain::SrtmDataset;
use small_world::wgs84::{Enu, Ned};

fn main() -> Result<(), Box<dyn Error>> {
    let egm96 = EGM96::new(Path::new("data/WW15MGH.DAC"))?;
    let egm2008 = EGM2008::new(Path::new("data/EGM2008_2_5.DAC"))?;
    let terrain = SrtmDataset::new("data/srtm"); // must contain SRTM .hgt tiles for your origin
    let c96 = AltitudeConverter::new(&egm96, &terrain);
    let c08 = AltitudeConverter::new(&egm2008, &terrain);

    // EXAMPLE 1: ENU point at MSL(EGM2008) origin -> NED point at MSL(EGM96) origin.
    let enu_origin =
        c08.lla_wgs84_from_height_m(GeoPoint::new(39.0000, -77.0000)?, 110.0, VerticalFrame::Msl)?;
    let ned_origin =
        c96.lla_wgs84_from_height_m(GeoPoint::new(39.0005, -77.0008)?, 120.0, VerticalFrame::Msl)?;
    let ned_point_m = Enu::new(15.0, -4.0, 3.0, enu_origin).to_ned(ned_origin);
    println!(
        "ENU->NED (m): n={:.3}, e={:.3}, d={:.3}",
        ned_point_m.n(),
        ned_point_m.e(),
        ned_point_m.d()
    );

    // EXAMPLE 2: NED point with AGL origin -> absolute LLA in HAE(WGS84).
    let ned_origin = c96.lla_wgs84_from_height_m(
        GeoPoint::new(39.0010, -77.0020)?,
        25.0, // origin altitude is 25 m AGL
        VerticalFrame::Agl,
    )?;
    let target_lla_hae_wgs84 = Ned::new(40.0, -8.0, 6.0, ned_origin).to_lla();
    println!(
        "NED->LLA (WGS84/HAE): lat={:.8}, lon={:.8}, hae_m={:.3}",
        target_lla_hae_wgs84.lat_deg(),
        target_lla_hae_wgs84.lon_deg(),
        target_lla_hae_wgs84.alt_m()
    );

    // EXAMPLE 3: altitude sample with explicit frame -> absolute ECEF, then back to MSL.
    let point = GeoPoint::new(39.0000, -77.0000)?;
    let point_ecef = c96.ecef_wgs84_from_height_m(point, 110.0, VerticalFrame::Msl)?;
    let recovered_msl_m = c96.height_from_ecef_wgs84_m(point_ecef, VerticalFrame::Msl)?;
    println!(
        "MSL->ECEF->MSL: x={:.3}, y={:.3}, z={:.3}, msl_m={:.3}",
        point_ecef.x(),
        point_ecef.y(),
        point_ecef.z(),
        recovered_msl_m
    );

    Ok(())
}
