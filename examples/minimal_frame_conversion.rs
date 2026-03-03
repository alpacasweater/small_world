use std::error::Error;
use std::path::Path;

use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::egm96::{EGM2008, EGM96};
use small_world::terrain::SrtmDataset;
use small_world::wgs84::{
    enu_to_ned_between_origins, ned_to_lla_wgs84, EnuMeters, LlaWgs84, NedMeters,
};

fn main() -> Result<(), Box<dyn Error>> {
    let egm96 = EGM96::new(Path::new("data/WW15MGH.DAC"))?;
    let egm2008 = EGM2008::new(Path::new("data/EGM2008_2_5.DAC"))?;
    let terrain = SrtmDataset::new("data/srtm"); // must contain SRTM .hgt tiles for your origin

    // EXAMPLE 1: ENU point at MSL(EGM2008) origin -> NED point at MSL(EGM96) origin.
    let enu_origin_msl_egm2008 = (39.0000, -77.0000, 110.0);
    let ned_origin_msl_egm96 = (39.0005, -77.0008, 120.0);
    let enu_origin_hae_wgs84 = LlaWgs84::new(
        enu_origin_msl_egm2008.0,
        enu_origin_msl_egm2008.1,
        enu_origin_msl_egm2008.2
            + egm2008.offset_bilinear(enu_origin_msl_egm2008.0, enu_origin_msl_egm2008.1)?,
    );
    let ned_origin_hae_wgs84 = LlaWgs84::new(
        ned_origin_msl_egm96.0,
        ned_origin_msl_egm96.1,
        ned_origin_msl_egm96.2
            + egm96.offset_bilinear(ned_origin_msl_egm96.0, ned_origin_msl_egm96.1)?,
    );
    let enu_point_m = EnuMeters::new(15.0, -4.0, 3.0);
    let ned_point_m =
        enu_to_ned_between_origins(enu_point_m, enu_origin_hae_wgs84, ned_origin_hae_wgs84);
    println!("ENU->NED (m): {:?}", ned_point_m);

    // EXAMPLE 2: NED point with AGL origin -> absolute LLA in HAE(WGS84).
    let converter = AltitudeConverter::new(&egm96, &terrain);
    let ned_origin_ll = GeoPoint::new(39.0010, -77.0020)?;
    let ned_origin_hae_m = converter.convert_height_m(
        ned_origin_ll,
        25.0, // origin altitude is 25 m AGL
        VerticalFrame::Agl,
        VerticalFrame::Hae,
    )?;
    let ned_origin_hae_wgs84 = LlaWgs84::new(
        ned_origin_ll.lat_deg,
        ned_origin_ll.lon_deg,
        ned_origin_hae_m,
    );
    let ned_from_origin_m = NedMeters::new(40.0, -8.0, 6.0);
    let target_lla_hae_wgs84 = ned_to_lla_wgs84(ned_from_origin_m, ned_origin_hae_wgs84);
    println!(
        "NED->LLA (WGS84/HAE): lat={:.8}, lon={:.8}, hae_m={:.3}",
        target_lla_hae_wgs84.lat_deg, target_lla_hae_wgs84.lon_deg, target_lla_hae_wgs84.hae_m
    );

    Ok(())
}
