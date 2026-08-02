use small_world::altitude::{
    AltitudeConverter, AltitudeError, AltitudeSample, EgmModel, GeoPoint, GeoidProvider,
    TerrainProvider, VerticalFrame,
};
use small_world::height::Interpolation;

const SAMPLE_COUNT: usize = 20_000;
const HEIGHT_TOLERANCE_M: f64 = 1e-9;
const ROUND_TRIP_TOLERANCE_M: f64 = 1e-9;
const LLA_TOLERANCE_M: f64 = 1e-9;

struct ConstGeoid {
    offset_m: f64,
}

impl GeoidProvider for ConstGeoid {
    fn model(&self) -> EgmModel {
        EgmModel::Egm96
    }

    fn geoid_offset_m(
        &self,
        _lat_deg: f64,
        _lon_deg: f64,
        _interpolation: Interpolation,
    ) -> Result<f64, AltitudeError> {
        Ok(self.offset_m)
    }
}

struct ConstTerrain {
    msl_m: f64,
}

impl TerrainProvider for ConstTerrain {
    fn vertical_datum(&self) -> EgmModel {
        EgmModel::Egm96
    }

    fn terrain_msl_m(
        &self,
        _lat_deg: f64,
        _lon_deg: f64,
        _interpolation: Interpolation,
    ) -> Result<f64, AltitudeError> {
        Ok(self.msl_m)
    }
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

fn frame_from_index(index: usize) -> VerticalFrame {
    match index {
        0 => VerticalFrame::Agl,
        1 => VerticalFrame::Msl(EgmModel::Egm96),
        _ => VerticalFrame::Hae,
    }
}

fn oracle_convert(
    value_m: f64,
    source: VerticalFrame,
    target: VerticalFrame,
    n_m: f64,
    g_m: f64,
) -> f64 {
    let msl_m = match source {
        VerticalFrame::Agl => value_m + g_m,
        VerticalFrame::Msl(_) => value_m,
        VerticalFrame::Hae => value_m - n_m,
    };
    match target {
        VerticalFrame::Agl => msl_m - g_m,
        VerticalFrame::Msl(_) => msl_m,
        VerticalFrame::Hae => msl_m + n_m,
    }
}

#[test]
fn altitude_frame_conversions_match_closed_form_oracle() -> Result<(), String> {
    let mut seed = 0xE703_7ED1_A0B4_28DB;
    let mut max_abs_err = 0.0_f64;
    let mut max_round_trip_err = 0.0_f64;
    let mut max_lla_alt_err = 0.0_f64;

    for i in 0..SAMPLE_COUNT {
        let lat = uniform(&mut seed, -90.0, 90.0);
        let lon = uniform(&mut seed, -720.0, 720.0);
        let geoid_offset_m = uniform(&mut seed, -120.0, 120.0);
        let ground_msl_m = uniform(&mut seed, -500.0, 9000.0);
        let value_m = uniform(&mut seed, -2000.0, 50_000.0);

        let source = frame_from_index((i / 3) % 3);
        let target = frame_from_index(i % 3);

        let geoid = ConstGeoid {
            offset_m: geoid_offset_m,
        };
        let terrain = ConstTerrain {
            msl_m: ground_msl_m,
        };
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(lat, lon).map_err(|err| err.to_string())?;

        let expected = oracle_convert(value_m, source, target, geoid_offset_m, ground_msl_m);
        let actual = converter
            .convert_height_m(point, value_m, source, target)
            .map_err(|err| err.to_string())?;
        let abs_err = (actual - expected).abs();
        max_abs_err = max_abs_err.max(abs_err);
        if abs_err > HEIGHT_TOLERANCE_M {
            return Err(format!(
                "height mismatch above tolerance: err={abs_err:.6e} m source={source:?} target={target:?} \
                 expected={expected:.12} actual={actual:.12} geoid={geoid_offset_m:.6} ground_msl={ground_msl_m:.6}"
            ));
        }

        let round_tripped = converter
            .convert_height_m(point, actual, target, source)
            .map_err(|err| err.to_string())?;
        let round_trip_err = (round_tripped - value_m).abs();
        max_round_trip_err = max_round_trip_err.max(round_trip_err);
        if round_trip_err > ROUND_TRIP_TOLERANCE_M {
            return Err(format!(
                "round-trip mismatch above tolerance: err={round_trip_err:.6e} m source={source:?} target={target:?} \
                 start={value_m:.12} round_tripped={round_tripped:.12}"
            ));
        }

        let lla = converter
            .lla_wgs84_from_sample(point, AltitudeSample::new(value_m, source).unwrap())
            .map_err(|err| err.to_string())?;
        let expected_hae = oracle_convert(
            value_m,
            source,
            VerticalFrame::Hae,
            geoid_offset_m,
            ground_msl_m,
        );
        let lla_alt_err = (lla.alt_m() - expected_hae).abs();
        max_lla_alt_err = max_lla_alt_err.max(lla_alt_err);
        if lla_alt_err > LLA_TOLERANCE_M {
            return Err(format!(
                "LLA altitude mismatch above tolerance: err={lla_alt_err:.6e} m source={source:?} \
                 expected_hae={expected_hae:.12} actual_hae={:.12}",
                lla.alt_m()
            ));
        }
    }

    eprintln!(
        "max AGL/MSL/HAE height error vs analytic oracle: {max_abs_err:.6e} m ({} samples)",
        SAMPLE_COUNT
    );
    eprintln!("max AGL/MSL/HAE round-trip error: {max_round_trip_err:.6e} m");
    eprintln!("max lla_wgs84_from_sample altitude error: {max_lla_alt_err:.6e} m");
    Ok(())
}
