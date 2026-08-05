//! Differential oracle against NGA's official EGM2008 interpolation test vectors.
//!
//! The EGM2008 interpolation package (`egm-08interpolation` from earth-info.nga.mil) ships
//! `INPUT.DAT`/`OUTPUT3.DAT`: six coordinate pairs with the undulations NGA's own
//! `interp_2p5min.f` produces from the 2.5-arc-minute grid, documented to match harmonic
//! synthesis within 1 cm. Those pairs are inlined below as the reference.
//!
//! These tests need the real grids staged locally (`scripts/download_geoid_data.sh`) and skip
//! with a note when `data/EGM2008_2_5.DAC` is absent, so plain `cargo test` stays green on a
//! fresh checkout.

use std::path::Path;

use small_world::altitude::{EgmModel, GeoPoint, GeoidShift};
use small_world::geoid::{EGM2008, EGM96};

const EGM96_PATH: &str = "data/WW15MGH.DAC";
const EGM2008_PATH: &str = "data/EGM2008_2_5.DAC";

/// (lat, lon, undulation_m) from the package's INPUT.DAT / OUTPUT3.DAT.
///
/// Rows 1, 2, 4, 5, 6 sit on 2.5′ grid nodes (row 2 re-states row 1 with negative longitude;
/// rows 5–6 sit on the constant south-pole row), so any sane interpolation must reproduce them
/// to rounding. Row 3's longitude falls between nodes and genuinely exercises interpolation.
const NGA_VECTORS: &[(f64, f64, f64)] = &[
    (37.0, 241.0, -26.151),
    (37.0, -119.0, -26.151),
    (36.0, 242.983333, -29.171),
    (90.0, 0.0, 14.899),
    (-90.0, 359.983333, -30.150),
    (-90.0, 0.0, -30.150),
];

const ON_NODE_TOLERANCE_M: f64 = 0.002;
const BILINEAR_OFF_NODE_TOLERANCE_M: f64 = 0.04;
const BICUBIC_OFF_NODE_TOLERANCE_M: f64 = 0.01;

fn real_egm2008() -> Option<EGM2008> {
    if !Path::new(EGM2008_PATH).exists() {
        eprintln!(
            "skipping real-geoid oracle test: {EGM2008_PATH} not staged \
             (run scripts/download_geoid_data.sh --model egm2008)"
        );
        return None;
    }
    Some(EGM2008::new(EGM2008_PATH).expect("staged EGM2008 grid should open"))
}

fn is_on_node(lat: f64, lon: f64) -> bool {
    // 2.5 arc-minutes = 1/24 degree. The south-pole row is constant, so longitude
    // interpolation there is exact regardless of the longitude sample.
    let node = |deg: f64| (deg * 24.0 - (deg * 24.0).round()).abs() < 1e-6;
    (node(lat) && node(lon)) || lat.abs() == 90.0
}

#[test]
fn egm2008_matches_nga_interpolation_vectors() {
    let Some(egm2008) = real_egm2008() else {
        return;
    };

    for &(lat, lon, expected) in NGA_VECTORS {
        let bilinear = egm2008.offset_bilinear(lat, lon).expect("in-range query");
        let bicubic = egm2008.offset_bicubic(lat, lon).expect("in-range query");
        let (bilinear_tol, bicubic_tol) = if is_on_node(lat, lon) {
            (ON_NODE_TOLERANCE_M, ON_NODE_TOLERANCE_M)
        } else {
            (BILINEAR_OFF_NODE_TOLERANCE_M, BICUBIC_OFF_NODE_TOLERANCE_M)
        };
        assert!(
            (bilinear - expected).abs() <= bilinear_tol,
            "bilinear N({lat}, {lon}) = {bilinear}, NGA reference {expected}"
        );
        assert!(
            (bicubic - expected).abs() <= bicubic_tol,
            "bicubic N({lat}, {lon}) = {bicubic}, NGA reference {expected}"
        );
    }
}

#[test]
fn egm96_and_egm2008_agree_to_model_difference_scale() {
    let Some(egm2008) = real_egm2008() else {
        return;
    };
    let egm96 = EGM96::new(EGM96_PATH).expect("repo-staged EGM96 grid should open");

    // The two models describe the same physical geoid; differences are decimetre-scale over
    // well-surveyed regions but reach several metres where EGM96 was poorly constrained
    // (observed in this sweep: 3.7 m at 80°S). A coarse global sweep bounds the pairwise
    // difference and its RMS, which would still catch any decoding error (endianness, record
    // framing, units) instantly — those produce tens of metres everywhere.
    let mut count = 0usize;
    let mut sum_sq = 0.0f64;
    for lat_step in -8..=8 {
        for lon_step in 0..24 {
            let lat = f64::from(lat_step) * 10.0;
            let lon = f64::from(lon_step) * 15.0;
            let n96 = egm96.offset_bilinear(lat, lon).expect("in-range query");
            let n2008 = egm2008.offset_bilinear(lat, lon).expect("in-range query");
            let diff = n96 - n2008;
            assert!(
                diff.abs() < 10.0,
                "N_EGM96({lat}, {lon}) = {n96} vs N_EGM2008 = {n2008}: |diff| ≥ 10 m"
            );
            sum_sq += diff * diff;
            count += 1;
        }
    }
    let rms = (sum_sq / count as f64).sqrt();
    assert!(
        rms < 1.0,
        "global EGM96↔EGM2008 RMS difference {rms} m ≥ 1 m"
    );
}

#[test]
fn geoid_shift_between_real_models_preserves_hae() {
    let Some(egm2008) = real_egm2008() else {
        return;
    };
    let egm96 = EGM96::new(EGM96_PATH).expect("repo-staged EGM96 grid should open");

    // MSL(to) = MSL(from) + N(from) − N(to): the same physical point keeps its HAE across the
    // re-referencing, so MSL + N must be invariant under the shift, and the round trip must
    // return the original height exactly (pure arithmetic on the same two lookups).
    let shift = GeoidShift::new(&egm96, &egm2008);
    assert_eq!(shift.from_model(), EgmModel::Egm96);
    assert_eq!(shift.to_model(), EgmModel::Egm2008);
    let reverse = GeoidShift::new(&egm2008, &egm96);

    for &(lat, lon, _) in &[
        (51.4779, -0.0015, 0.0),
        (27.9881, 86.925, 0.0),
        (0.0, 0.0, 0.0),
    ] {
        let point = GeoPoint::new(lat, lon).expect("valid point");
        let msl96 = 123.456;
        let msl2008 = shift
            .convert_height_m(point, msl96)
            .expect("shift in range");

        let n96 = egm96.offset_bilinear(lat, lon).expect("in-range query");
        let n2008 = egm2008.offset_bilinear(lat, lon).expect("in-range query");
        let hae_via_96 = msl96 + n96;
        let hae_via_2008 = msl2008 + n2008;
        assert!(
            (hae_via_96 - hae_via_2008).abs() < 1e-9,
            "HAE not preserved at ({lat}, {lon}): {hae_via_96} vs {hae_via_2008}"
        );

        let msl96_back = reverse
            .convert_height_m(point, msl2008)
            .expect("shift in range");
        assert!(
            (msl96_back - msl96).abs() < 1e-9,
            "round trip drifted at ({lat}, {lon}): {msl96_back} vs {msl96}"
        );
    }
}
