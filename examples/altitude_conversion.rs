use std::env;
use std::path::Path;
use std::time::Instant;

use small_world::altitude::{
    AltitudeConverter, AltitudeError, AltitudeSample, GeoPoint, VerticalFrame,
};
use small_world::geoid::{EGM2008, EGM96};
use small_world::height::Interpolation;
use small_world::terrain::SrtmDataset;

fn print_usage() {
    println!("Usage:");
    println!("  cargo run --example altitude_conversion -- <egm96|egm2008> <geoid_path> <srtm_dir> <lat_deg> <lon_deg> <value_m> <from:agl|msl|hae> <to:agl|msl|hae> [nearest|bilinear|bicubic]");
    println!("Example:");
    println!("  cargo run --example altitude_conversion -- egm96 data/WW15MGH.DAC data/srtm 39.0 -77.0 120 agl hae bilinear");
}

fn parse_height_ref(value: &str) -> Option<VerticalFrame> {
    match value.to_ascii_lowercase().as_str() {
        "agl" => Some(VerticalFrame::Agl),
        "msl" => Some(VerticalFrame::Msl),
        "hae" => Some(VerticalFrame::Hae),
        _ => None,
    }
}

fn frame_name(frame: VerticalFrame) -> &'static str {
    match frame {
        VerticalFrame::Agl => "AGL (m above terrain)",
        VerticalFrame::Msl => "MSL (orthometric meters)",
        VerticalFrame::Hae => "HAE (ellipsoidal meters above WGS84)",
    }
}

fn parse_interpolation(value: Option<&str>) -> Option<Interpolation> {
    match value.unwrap_or("bilinear").to_ascii_lowercase().as_str() {
        "nearest" => Some(Interpolation::Nearest),
        "bilinear" => Some(Interpolation::Bilinear),
        "bicubic" => Some(Interpolation::Bicubic),
        _ => None,
    }
}

fn run_with_geoid<G>(
    geoid: &G,
    terrain: &SrtmDataset,
    point: GeoPoint,
    sample: AltitudeSample,
    target_frame: VerticalFrame,
    interpolation: Interpolation,
) -> Result<(AltitudeSample, f64, f64, f64), AltitudeError>
where
    G: small_world::altitude::GeoidProvider + ?Sized,
{
    let converter = AltitudeConverter::new(geoid, terrain)
        .with_geoid_interpolation(interpolation)
        .with_terrain_interpolation(interpolation);

    let output_sample = converter.convert_sample(point, sample, target_frame)?;
    let reference = converter.reference_at(point)?;
    Ok((
        output_sample,
        reference.geoid_offset_m,
        reference.ground_msl_m,
        reference.ground_hae_m,
    ))
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 9 {
        print_usage();
        return;
    }

    let model = args[1].to_ascii_lowercase();
    let geoid_path = Path::new(&args[2]);
    let srtm_root = Path::new(&args[3]);
    let lat_deg: f64 = args[4].parse().expect("lat_deg must be a float");
    let lon_deg: f64 = args[5].parse().expect("lon_deg must be a float");
    let value_m: f64 = args[6].parse().expect("value_m must be a float");
    let source_frame = parse_height_ref(&args[7]).expect("invalid from reference (agl|msl|hae)");
    let target_frame = parse_height_ref(&args[8]).expect("invalid to reference (agl|msl|hae)");
    let interpolation =
        parse_interpolation(args.get(9).map(String::as_str)).expect("invalid interpolation mode");
    let point = GeoPoint::new(lat_deg, lon_deg).expect("invalid geodetic point");
    let sample = AltitudeSample::new(value_m, source_frame).expect("invalid altitude input");

    let terrain = SrtmDataset::new(srtm_root);
    let start = Instant::now();

    let result = match model.as_str() {
        "egm96" => {
            let geoid = EGM96::new(geoid_path).expect("failed to load EGM96 dataset");
            run_with_geoid(&geoid, &terrain, point, sample, target_frame, interpolation)
        }
        "egm2008" => {
            let geoid = EGM2008::new(geoid_path).expect("failed to load EGM2008 dataset");
            run_with_geoid(&geoid, &terrain, point, sample, target_frame, interpolation)
        }
        _ => {
            eprintln!("unknown geoid model: {model}");
            print_usage();
            return;
        }
    };

    match result {
        Ok((value_out, geoid_offset_m, ground_msl_m, ground_hae_m)) => {
            let elapsed = start.elapsed();
            println!("Model: {model}");
            println!("Geoid dataset: {}", geoid_path.display());
            println!("Terrain root: {}", srtm_root.display());
            println!("Input location: lat={lat_deg:.8}, lon={lon_deg:.8}");
            println!("Input altitude: {value_m:.3} {}", frame_name(source_frame));
            println!(
                "Converted altitude: {:.3} {}",
                value_out.meters,
                frame_name(value_out.frame)
            );
            println!("Geoid offset N (MSL->HAE): {geoid_offset_m:.3} m");
            println!("Ground terrain elevation: {ground_msl_m:.3} m MSL");
            println!("Ground terrain elevation: {ground_hae_m:.3} m HAE");
            println!("Query time: {:?}", elapsed);
        }
        Err(err) => eprintln!("conversion failed: {err}"),
    }
}
