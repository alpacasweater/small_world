//! Altitude conversion primitives with explicit vertical frame semantics.
//!
//! This module intentionally requires explicit source/target frames for every conversion.
//! Use [`crate::altitude::AltitudeConverter::convert_height_m`] as the primary API for scalar
//! values.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::egm96::{EgmError, EGM2008, EGM96};
use crate::height::Interpolation;
use crate::terrain::{SrtmDataset, TerrainError};
use crate::wgs84::{AltType, Lla};

/// Vertical reference frame for altitude values in this crate.
///
/// All heights are in meters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerticalFrame {
    /// Height above local terrain surface from the DEM (`terrain_msl`).
    Agl,
    /// Orthometric height above mean sea level (MSL).
    Msl,
    /// Ellipsoidal height above the WGS84 reference ellipsoid (HAE).
    Hae,
}

/// Geodetic query location in decimal degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPoint {
    pub lat_deg: f64,
    pub lon_deg: f64,
}

impl GeoPoint {
    /// Creates a geodetic point in decimal degrees.
    ///
    /// Constraints:
    /// - `lat_deg` must be finite and in `[-90, 90]`
    /// - `lon_deg` must be finite (normalization is backend-specific)
    pub fn new(lat_deg: f64, lon_deg: f64) -> Result<Self, AltitudeError> {
        validate_latitude(lat_deg)?;
        validate_longitude(lon_deg)?;
        Ok(Self { lat_deg, lon_deg })
    }
}

/// Altitude sample with explicit vertical frame and meter units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AltitudeSample {
    pub meters: f64,
    pub frame: VerticalFrame,
}

impl AltitudeSample {
    /// Creates a typed altitude sample in meters with an explicit frame.
    pub fn new(meters: f64, frame: VerticalFrame) -> Result<Self, AltitudeError> {
        validate_height("meters", meters)?;
        Ok(Self { meters, frame })
    }

    /// Convenience constructor for an `AGL` sample in meters.
    pub fn agl_m(meters: f64) -> Result<Self, AltitudeError> {
        Self::new(meters, VerticalFrame::Agl)
    }

    /// Convenience constructor for an `MSL` sample in meters.
    pub fn msl_m(meters: f64) -> Result<Self, AltitudeError> {
        Self::new(meters, VerticalFrame::Msl)
    }

    /// Convenience constructor for an `HAE` sample in meters.
    pub fn hae_m(meters: f64) -> Result<Self, AltitudeError> {
        Self::new(meters, VerticalFrame::Hae)
    }
}

/// Errors returned by altitude conversion and reference queries.
#[derive(Debug)]
pub enum AltitudeError {
    /// Failed geoid lookup or interpolation.
    Geoid(EgmError),
    /// Failed terrain lookup or interpolation.
    Terrain(TerrainError),
    /// Invalid geographic coordinate argument.
    InvalidCoordinate { name: &'static str, value: f64 },
    /// Invalid altitude/height argument.
    InvalidHeight { name: &'static str, value: f64 },
}

impl Display for AltitudeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AltitudeError::Geoid(err) => write!(f, "geoid query failed: {err}"),
            AltitudeError::Terrain(err) => write!(f, "terrain query failed: {err}"),
            AltitudeError::InvalidCoordinate { name, value } => {
                write!(f, "{name} must be finite and in valid bounds, got {value}")
            }
            AltitudeError::InvalidHeight { name, value } => {
                write!(f, "{name} must be finite, got {value}")
            }
        }
    }
}

impl Error for AltitudeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AltitudeError::Geoid(err) => Some(err),
            AltitudeError::Terrain(err) => Some(err),
            AltitudeError::InvalidCoordinate { .. } => None,
            AltitudeError::InvalidHeight { .. } => None,
        }
    }
}

impl From<EgmError> for AltitudeError {
    fn from(value: EgmError) -> Self {
        AltitudeError::Geoid(value)
    }
}

impl From<TerrainError> for AltitudeError {
    fn from(value: TerrainError) -> Self {
        AltitudeError::Terrain(value)
    }
}

/// Interface for geoid separation providers (`N` in `HAE = MSL + N`).
pub trait GeoidProvider {
    /// Returns geoid separation in meters at `(lat_deg, lon_deg)`.
    fn geoid_offset_m(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        interpolation: Interpolation,
    ) -> Result<f64, AltitudeError>;
}

/// Interface for terrain providers (`ground_msl` in `MSL = ground_msl + AGL`).
pub trait TerrainProvider {
    /// Returns terrain elevation in orthometric MSL meters at `(lat_deg, lon_deg)`.
    fn terrain_msl_m(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        interpolation: Interpolation,
    ) -> Result<f64, AltitudeError>;
}

impl GeoidProvider for EGM96 {
    fn geoid_offset_m(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        interpolation: Interpolation,
    ) -> Result<f64, AltitudeError> {
        match interpolation {
            Interpolation::Nearest => self.offset(lat_deg, lon_deg),
            Interpolation::Bilinear => self.offset_bilinear(lat_deg, lon_deg),
            Interpolation::Bicubic => self.offset_bicubic(lat_deg, lon_deg),
        }
        .map_err(AltitudeError::from)
    }
}

impl GeoidProvider for EGM2008 {
    fn geoid_offset_m(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        interpolation: Interpolation,
    ) -> Result<f64, AltitudeError> {
        match interpolation {
            Interpolation::Nearest => self.offset(lat_deg, lon_deg),
            Interpolation::Bilinear => self.offset_bilinear(lat_deg, lon_deg),
            Interpolation::Bicubic => self.offset_bicubic(lat_deg, lon_deg),
        }
        .map_err(AltitudeError::from)
    }
}

impl TerrainProvider for SrtmDataset {
    fn terrain_msl_m(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        interpolation: Interpolation,
    ) -> Result<f64, AltitudeError> {
        self.elevation_msl_with_interpolation(lat_deg, lon_deg, interpolation)
            .map_err(AltitudeError::from)
    }
}

/// Terrain/geoid reference terms for a geodetic point.
#[derive(Clone, Copy, Debug)]
pub struct TerrainReference {
    /// Geoid separation (`N`) where `HAE = MSL + N`.
    pub geoid_offset_m: f64,
    /// DEM terrain elevation in orthometric MSL meters.
    pub ground_msl_m: f64,
    /// DEM terrain elevation in ellipsoidal HAE meters.
    pub ground_hae_m: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct AltitudeConverter<'a, G, T>
where
    G: GeoidProvider + ?Sized,
    T: TerrainProvider + ?Sized,
{
    geoid: &'a G,
    terrain: &'a T,
    geoid_interpolation: Interpolation,
    terrain_interpolation: Interpolation,
}

impl<'a, G, T> AltitudeConverter<'a, G, T>
where
    G: GeoidProvider + ?Sized,
    T: TerrainProvider + ?Sized,
{
    /// Creates a converter that combines:
    /// - geoid separation (`MSL <-> HAE`)
    /// - terrain elevation (`MSL <-> AGL`)
    pub fn new(geoid: &'a G, terrain: &'a T) -> Self {
        Self {
            geoid,
            terrain,
            geoid_interpolation: Interpolation::Bilinear,
            terrain_interpolation: Interpolation::Bilinear,
        }
    }

    /// Sets interpolation mode for geoid offset queries.
    pub fn with_geoid_interpolation(mut self, interpolation: Interpolation) -> Self {
        self.geoid_interpolation = interpolation;
        self
    }

    /// Sets interpolation mode for terrain elevation queries.
    pub fn with_terrain_interpolation(mut self, interpolation: Interpolation) -> Self {
        self.terrain_interpolation = interpolation;
        self
    }

    /// Geoid separation (`N`) in meters at `(lat_deg, lon_deg)`.
    pub fn geoid_offset_m(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, AltitudeError> {
        self.geoid
            .geoid_offset_m(lat_deg, lon_deg, self.geoid_interpolation)
    }

    /// Terrain orthometric elevation in MSL meters at `(lat_deg, lon_deg)`.
    pub fn ground_msl_m(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, AltitudeError> {
        self.terrain
            .terrain_msl_m(lat_deg, lon_deg, self.terrain_interpolation)
    }

    /// Returns all base reference terms used for frame conversion at `(lat_deg, lon_deg)`.
    pub fn reference(&self, lat_deg: f64, lon_deg: f64) -> Result<TerrainReference, AltitudeError> {
        let geoid_offset_m = self.geoid_offset_m(lat_deg, lon_deg)?;
        let ground_msl_m = self.ground_msl_m(lat_deg, lon_deg)?;
        let ground_hae_m = ground_msl_m + geoid_offset_m;
        Ok(TerrainReference {
            geoid_offset_m,
            ground_msl_m,
            ground_hae_m,
        })
    }

    /// Point-typed variant of [`Self::reference`].
    pub fn reference_at(&self, point: GeoPoint) -> Result<TerrainReference, AltitudeError> {
        self.reference(point.lat_deg, point.lon_deg)
    }

    /// Converts an altitude sample from one explicit vertical frame to another.
    pub fn convert_sample(
        &self,
        point: GeoPoint,
        sample: AltitudeSample,
        target_frame: VerticalFrame,
    ) -> Result<AltitudeSample, AltitudeError> {
        if sample.frame == target_frame {
            return AltitudeSample::new(sample.meters, sample.frame);
        }

        let msl_m = match sample.frame {
            VerticalFrame::Agl => self.msl_from_agl(point.lat_deg, point.lon_deg, sample.meters)?,
            VerticalFrame::Msl => sample.meters,
            VerticalFrame::Hae => self.msl_from_hae(point.lat_deg, point.lon_deg, sample.meters)?,
        };

        let meters = match target_frame {
            VerticalFrame::Agl => self.agl_from_msl(point.lat_deg, point.lon_deg, msl_m)?,
            VerticalFrame::Msl => msl_m,
            VerticalFrame::Hae => self.hae_from_msl(point.lat_deg, point.lon_deg, msl_m)?,
        };

        AltitudeSample::new(meters, target_frame)
    }

    /// Convenience conversion API for scalar meter values when source/target frames are explicit.
    pub fn convert_height_m(
        &self,
        point: GeoPoint,
        meters: f64,
        source_frame: VerticalFrame,
        target_frame: VerticalFrame,
    ) -> Result<f64, AltitudeError> {
        let sample = AltitudeSample::new(meters, source_frame)?;
        Ok(self.convert_sample(point, sample, target_frame)?.meters)
    }

    /// Converts a height sample at `point` into absolute geodetic LLA in WGS84/HAE.
    ///
    /// This is a high-level convenience for callers that need `Lla` directly
    /// from explicit source frame data (`AGL`, `MSL`, or `HAE`).
    pub fn lla_wgs84_from_height_m(
        &self,
        point: GeoPoint,
        meters: f64,
        source_frame: VerticalFrame,
    ) -> Result<Lla, AltitudeError> {
        let hae_m = self.convert_height_m(point, meters, source_frame, VerticalFrame::Hae)?;
        Ok(Lla::new(
            point.lat_deg,
            point.lon_deg,
            hae_m,
            AltType::Wgs84,
        ))
    }

    /// Typed variant of [`Self::lla_wgs84_from_height_m`] using an [`AltitudeSample`].
    pub fn lla_wgs84_from_sample(
        &self,
        point: GeoPoint,
        sample: AltitudeSample,
    ) -> Result<Lla, AltitudeError> {
        let hae = self.convert_sample(point, sample, VerticalFrame::Hae)?;
        Ok(Lla::new(
            point.lat_deg,
            point.lon_deg,
            hae.meters,
            AltType::Wgs84,
        ))
    }

    /// Converts orthometric `MSL` meters into WGS84 ellipsoidal `HAE` meters.
    pub fn hae_from_msl(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        msl_m: f64,
    ) -> Result<f64, AltitudeError> {
        validate_height("msl_m", msl_m)?;
        let geoid_offset_m = self.geoid_offset_m(lat_deg, lon_deg)?;
        Ok(msl_m + geoid_offset_m)
    }

    /// Converts WGS84 ellipsoidal `HAE` meters into orthometric `MSL` meters.
    pub fn msl_from_hae(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        hae_m: f64,
    ) -> Result<f64, AltitudeError> {
        validate_height("hae_m", hae_m)?;
        let geoid_offset_m = self.geoid_offset_m(lat_deg, lon_deg)?;
        Ok(hae_m - geoid_offset_m)
    }

    /// Converts local terrain-relative `AGL` meters into orthometric `MSL` meters.
    pub fn msl_from_agl(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        agl_m: f64,
    ) -> Result<f64, AltitudeError> {
        validate_height("agl_m", agl_m)?;
        let ground_msl_m = self.ground_msl_m(lat_deg, lon_deg)?;
        Ok(ground_msl_m + agl_m)
    }

    /// Converts orthometric `MSL` meters into local terrain-relative `AGL` meters.
    pub fn agl_from_msl(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        msl_m: f64,
    ) -> Result<f64, AltitudeError> {
        validate_height("msl_m", msl_m)?;
        let ground_msl_m = self.ground_msl_m(lat_deg, lon_deg)?;
        Ok(msl_m - ground_msl_m)
    }

    /// Converts local terrain-relative `AGL` meters into WGS84 ellipsoidal `HAE` meters.
    pub fn hae_from_agl(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        agl_m: f64,
    ) -> Result<f64, AltitudeError> {
        let msl_m = self.msl_from_agl(lat_deg, lon_deg, agl_m)?;
        self.hae_from_msl(lat_deg, lon_deg, msl_m)
    }

    /// Converts WGS84 ellipsoidal `HAE` meters into local terrain-relative `AGL` meters.
    pub fn agl_from_hae(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        hae_m: f64,
    ) -> Result<f64, AltitudeError> {
        let msl_m = self.msl_from_hae(lat_deg, lon_deg, hae_m)?;
        self.agl_from_msl(lat_deg, lon_deg, msl_m)
    }
}

fn validate_latitude(value: f64) -> Result<(), AltitudeError> {
    if !value.is_finite() || !(-90.0..=90.0).contains(&value) {
        return Err(AltitudeError::InvalidCoordinate {
            name: "lat_deg",
            value,
        });
    }
    Ok(())
}

fn validate_longitude(value: f64) -> Result<(), AltitudeError> {
    if !value.is_finite() {
        return Err(AltitudeError::InvalidCoordinate {
            name: "lon_deg",
            value,
        });
    }
    Ok(())
}

fn validate_height(name: &'static str, value: f64) -> Result<(), AltitudeError> {
    if !value.is_finite() {
        return Err(AltitudeError::InvalidHeight { name, value });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use proptest::prelude::*;

    use crate::wgs84::AltType;

    use super::{
        AltitudeConverter, AltitudeError, AltitudeSample, GeoPoint, GeoidProvider, Interpolation,
        TerrainProvider, VerticalFrame,
    };

    struct MockGeoid {
        value_m: f64,
        interpolation_seen: Cell<Option<Interpolation>>,
        query_count: Cell<usize>,
    }

    impl MockGeoid {
        fn new(value_m: f64) -> Self {
            Self {
                value_m,
                interpolation_seen: Cell::new(None),
                query_count: Cell::new(0),
            }
        }

        fn query_count(&self) -> usize {
            self.query_count.get()
        }
    }

    impl GeoidProvider for MockGeoid {
        fn geoid_offset_m(
            &self,
            _lat_deg: f64,
            _lon_deg: f64,
            interpolation: Interpolation,
        ) -> Result<f64, AltitudeError> {
            self.interpolation_seen.set(Some(interpolation));
            self.query_count.set(self.query_count.get() + 1);
            Ok(self.value_m)
        }
    }

    struct MockTerrain {
        value_m: f64,
        interpolation_seen: Cell<Option<Interpolation>>,
        query_count: Cell<usize>,
    }

    impl MockTerrain {
        fn new(value_m: f64) -> Self {
            Self {
                value_m,
                interpolation_seen: Cell::new(None),
                query_count: Cell::new(0),
            }
        }

        fn query_count(&self) -> usize {
            self.query_count.get()
        }
    }

    impl TerrainProvider for MockTerrain {
        fn terrain_msl_m(
            &self,
            _lat_deg: f64,
            _lon_deg: f64,
            interpolation: Interpolation,
        ) -> Result<f64, AltitudeError> {
            self.interpolation_seen.set(Some(interpolation));
            self.query_count.set(self.query_count.get() + 1);
            Ok(self.value_m)
        }
    }

    #[test]
    fn agl_msl_hae_round_trip_is_consistent() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);

        let agl_m = 50.0;
        let msl_m = converter.msl_from_agl(10.0, 20.0, agl_m).unwrap();
        let hae_m = converter.hae_from_agl(10.0, 20.0, agl_m).unwrap();

        assert!((msl_m - 170.0).abs() < 1e-12);
        assert!((hae_m - 200.0).abs() < 1e-12);
        assert!((converter.agl_from_msl(10.0, 20.0, msl_m).unwrap() - agl_m).abs() < 1e-12);
        assert!((converter.agl_from_hae(10.0, 20.0, hae_m).unwrap() - agl_m).abs() < 1e-12);
        assert!((converter.msl_from_hae(10.0, 20.0, hae_m).unwrap() - msl_m).abs() < 1e-12);
    }

    #[test]
    fn reference_returns_ground_msl_and_hae() {
        let geoid = MockGeoid::new(-12.0);
        let terrain = MockTerrain::new(432.5);
        let converter = AltitudeConverter::new(&geoid, &terrain);

        let reference = converter.reference(45.0, -122.0).unwrap();
        assert!((reference.geoid_offset_m + 12.0).abs() < 1e-12);
        assert!((reference.ground_msl_m - 432.5).abs() < 1e-12);
        assert!((reference.ground_hae_m - 420.5).abs() < 1e-12);
    }

    #[test]
    fn configured_interpolation_modes_are_used() {
        let geoid = MockGeoid::new(10.0);
        let terrain = MockTerrain::new(20.0);
        let converter = AltitudeConverter::new(&geoid, &terrain)
            .with_geoid_interpolation(Interpolation::Bicubic)
            .with_terrain_interpolation(Interpolation::Nearest);

        let _ = converter.hae_from_agl(0.0, 0.0, 5.0).unwrap();
        assert_eq!(geoid.interpolation_seen.get(), Some(Interpolation::Bicubic));
        assert_eq!(
            terrain.interpolation_seen.get(),
            Some(Interpolation::Nearest)
        );
    }

    #[test]
    fn rejects_non_finite_height_inputs() {
        let geoid = MockGeoid::new(10.0);
        let terrain = MockTerrain::new(20.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);

        let err = converter.hae_from_msl(0.0, 0.0, f64::NAN).unwrap_err();
        assert!(matches!(err, AltitudeError::InvalidHeight { .. }));
    }

    #[test]
    fn altitude_sample_requires_explicit_frame_and_finite_value() {
        let sample = AltitudeSample::agl_m(25.0).unwrap();
        assert_eq!(sample.frame, VerticalFrame::Agl);
        assert!((sample.meters - 25.0).abs() < 1e-12);

        let err = AltitudeSample::hae_m(f64::NEG_INFINITY).unwrap_err();
        assert!(matches!(err, AltitudeError::InvalidHeight { .. }));
    }

    #[test]
    fn geodetic_point_validates_bounds() {
        assert!(GeoPoint::new(45.0, -122.0).is_ok());
        assert!(matches!(
            GeoPoint::new(91.0, 0.0).unwrap_err(),
            AltitudeError::InvalidCoordinate {
                name: "lat_deg",
                ..
            }
        ));
        assert!(matches!(
            GeoPoint::new(0.0, f64::NAN).unwrap_err(),
            AltitudeError::InvalidCoordinate {
                name: "lon_deg",
                ..
            }
        ));
    }

    #[test]
    fn typed_conversion_matrix_is_consistent() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let agl = AltitudeSample::agl_m(50.0).unwrap();
        let msl = converter
            .convert_sample(point, agl, VerticalFrame::Msl)
            .unwrap();
        let hae = converter
            .convert_sample(point, agl, VerticalFrame::Hae)
            .unwrap();

        assert_eq!(msl.frame, VerticalFrame::Msl);
        assert_eq!(hae.frame, VerticalFrame::Hae);
        assert!((msl.meters - 170.0).abs() < 1e-12);
        assert!((hae.meters - 200.0).abs() < 1e-12);

        let agl_back = converter
            .convert_sample(point, hae, VerticalFrame::Agl)
            .unwrap();
        assert_eq!(agl_back.frame, VerticalFrame::Agl);
        assert!((agl_back.meters - 50.0).abs() < 1e-12);
    }

    #[test]
    fn convert_height_matches_typed_sample_conversion() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let via_scalar =
            converter.convert_height_m(point, 200.0, VerticalFrame::Hae, VerticalFrame::Agl);
        let via_sample = converter.convert_sample(
            point,
            AltitudeSample::hae_m(200.0).unwrap(),
            VerticalFrame::Agl,
        );
        assert!((via_scalar.unwrap() - via_sample.unwrap().meters).abs() < 1e-12);
    }

    #[test]
    fn lla_wgs84_from_height_is_frame_explicit() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let lla = converter
            .lla_wgs84_from_height_m(point, 50.0, VerticalFrame::Agl)
            .unwrap();
        assert!((lla.lat_deg() - 10.0).abs() < 1e-12);
        assert!((lla.lon_deg() - 20.0).abs() < 1e-12);
        assert!((lla.alt_m() - 200.0).abs() < 1e-12);
        assert_eq!(lla.alt_type(), AltType::Wgs84);
    }

    #[test]
    fn lla_wgs84_from_sample_matches_scalar_helper() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let from_scalar = converter
            .lla_wgs84_from_height_m(point, 200.0, VerticalFrame::Hae)
            .unwrap();
        let from_sample = converter
            .lla_wgs84_from_sample(point, AltitudeSample::hae_m(200.0).unwrap())
            .unwrap();

        assert!((from_scalar.lat_deg() - from_sample.lat_deg()).abs() < 1e-12);
        assert!((from_scalar.lon_deg() - from_sample.lon_deg()).abs() < 1e-12);
        assert!((from_scalar.alt_m() - from_sample.alt_m()).abs() < 1e-12);
        assert_eq!(from_scalar.alt_type(), from_sample.alt_type());
    }

    #[test]
    fn same_frame_conversion_is_strict_identity_and_query_free() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let frames = [VerticalFrame::Agl, VerticalFrame::Msl, VerticalFrame::Hae];
        for frame in frames {
            let via_sample = converter
                .convert_sample(point, AltitudeSample::new(123.45, frame).unwrap(), frame)
                .unwrap();
            assert_eq!(via_sample.frame, frame);
            assert!((via_sample.meters - 123.45).abs() < 1e-12);

            let via_scalar = converter
                .convert_height_m(point, 123.45, frame, frame)
                .unwrap();
            assert!((via_scalar - 123.45).abs() < 1e-12);
        }

        assert_eq!(geoid.query_count(), 0);
        assert_eq!(terrain.query_count(), 0);
    }

    #[test]
    fn lla_from_hae_is_query_free_and_preserves_altitude() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let lla = converter
            .lla_wgs84_from_height_m(point, 200.0, VerticalFrame::Hae)
            .unwrap();
        assert!((lla.lat_deg() - 10.0).abs() < 1e-12);
        assert!((lla.lon_deg() - 20.0).abs() < 1e-12);
        assert!((lla.alt_m() - 200.0).abs() < 1e-12);
        assert_eq!(lla.alt_type(), AltType::Wgs84);
        assert_eq!(geoid.query_count(), 0);
        assert_eq!(terrain.query_count(), 0);
    }

    #[test]
    fn conversion_matrix_matches_closed_form_relationships() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let frames = [VerticalFrame::Agl, VerticalFrame::Msl, VerticalFrame::Hae];
        for source in frames {
            for target in frames {
                let input = 250.0;
                let expected_msl = match source {
                    VerticalFrame::Agl => input + 120.0,
                    VerticalFrame::Msl => input,
                    VerticalFrame::Hae => input - 30.0,
                };
                let expected = match target {
                    VerticalFrame::Agl => expected_msl - 120.0,
                    VerticalFrame::Msl => expected_msl,
                    VerticalFrame::Hae => expected_msl + 30.0,
                };

                let actual = converter
                    .convert_height_m(point, input, source, target)
                    .unwrap();
                assert!(
                    (actual - expected).abs() < 1e-12,
                    "source={source:?} target={target:?} expected={expected} actual={actual}"
                );
            }
        }
    }

    #[test]
    fn frame_pair_queries_use_only_required_references() {
        let point = GeoPoint::new(10.0, 20.0).unwrap();
        let cases = [
            (VerticalFrame::Agl, VerticalFrame::Agl, 0, 0),
            (VerticalFrame::Agl, VerticalFrame::Msl, 0, 1),
            (VerticalFrame::Agl, VerticalFrame::Hae, 1, 1),
            (VerticalFrame::Msl, VerticalFrame::Agl, 0, 1),
            (VerticalFrame::Msl, VerticalFrame::Msl, 0, 0),
            (VerticalFrame::Msl, VerticalFrame::Hae, 1, 0),
            (VerticalFrame::Hae, VerticalFrame::Agl, 1, 1),
            (VerticalFrame::Hae, VerticalFrame::Msl, 1, 0),
            (VerticalFrame::Hae, VerticalFrame::Hae, 0, 0),
        ];

        for (source, target, expected_geoid_queries, expected_terrain_queries) in cases {
            let geoid = MockGeoid::new(30.0);
            let terrain = MockTerrain::new(120.0);
            let converter = AltitudeConverter::new(&geoid, &terrain);
            let _ = converter
                .convert_height_m(point, 250.0, source, target)
                .unwrap();

            assert_eq!(
                geoid.query_count(),
                expected_geoid_queries,
                "source={source:?} target={target:?}"
            );
            assert_eq!(
                terrain.query_count(),
                expected_terrain_queries,
                "source={source:?} target={target:?}"
            );
        }
    }

    #[test]
    fn same_frame_fast_path_still_validates_finite_height() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        // Simulate external caller bypassing constructors via public fields.
        let invalid_sample = AltitudeSample {
            meters: f64::NAN,
            frame: VerticalFrame::Msl,
        };
        let err = converter
            .convert_sample(point, invalid_sample, VerticalFrame::Msl)
            .unwrap_err();
        assert!(matches!(
            err,
            AltitudeError::InvalidHeight { name: "meters", .. }
        ));

        // Same-frame failure should not have queried reference datasets.
        assert_eq!(geoid.query_count(), 0);
        assert_eq!(terrain.query_count(), 0);
    }

    proptest! {
        #[test]
        fn randomized_round_trip_invariants_hold(
            lat in -90.0f64..90.0,
            lon in -720.0f64..720.0,
            geoid_offset_m in -120.0f64..120.0,
            ground_msl_m in -500.0f64..9000.0,
            agl_m in -1000.0f64..50000.0,
        ) {
            let geoid = MockGeoid::new(geoid_offset_m);
            let terrain = MockTerrain::new(ground_msl_m);
            let converter = AltitudeConverter::new(&geoid, &terrain);
            let point = GeoPoint::new(lat, lon).unwrap();

            let agl = AltitudeSample::agl_m(agl_m).unwrap();
            let msl = converter.convert_sample(point, agl, VerticalFrame::Msl).unwrap();
            let hae = converter.convert_sample(point, msl, VerticalFrame::Hae).unwrap();
            let agl_back = converter.convert_sample(point, hae, VerticalFrame::Agl).unwrap();

            prop_assert_eq!(msl.frame, VerticalFrame::Msl);
            prop_assert_eq!(hae.frame, VerticalFrame::Hae);
            prop_assert_eq!(agl_back.frame, VerticalFrame::Agl);
            prop_assert!((agl_back.meters - agl_m).abs() < 1e-9);
            prop_assert!((hae.meters - msl.meters - geoid_offset_m).abs() < 1e-9);
            prop_assert!((msl.meters - ground_msl_m - agl_m).abs() < 1e-9);
            prop_assert!(msl.meters.is_finite());
            prop_assert!(hae.meters.is_finite());
            prop_assert!(agl_back.meters.is_finite());
        }
    }

    proptest! {
        #[test]
        fn scalar_and_typed_conversions_match_for_random_frames(
            lat in -90.0f64..90.0,
            lon in -720.0f64..720.0,
            geoid_offset_m in -120.0f64..120.0,
            ground_msl_m in -500.0f64..9000.0,
            value_m in -2000.0f64..50000.0,
            source in 0u8..3,
            target in 0u8..3,
        ) {
            let geoid = MockGeoid::new(geoid_offset_m);
            let terrain = MockTerrain::new(ground_msl_m);
            let converter = AltitudeConverter::new(&geoid, &terrain);
            let point = GeoPoint::new(lat, lon).unwrap();

            let source_frame = match source {
                0 => VerticalFrame::Agl,
                1 => VerticalFrame::Msl,
                _ => VerticalFrame::Hae,
            };
            let target_frame = match target {
                0 => VerticalFrame::Agl,
                1 => VerticalFrame::Msl,
                _ => VerticalFrame::Hae,
            };

            let typed = converter
                .convert_sample(
                    point,
                    AltitudeSample::new(value_m, source_frame).unwrap(),
                    target_frame,
                )
                .unwrap();
            let scalar = converter
                .convert_height_m(point, value_m, source_frame, target_frame)
                .unwrap();

            prop_assert_eq!(typed.frame, target_frame);
            prop_assert!((typed.meters - scalar).abs() < 1e-9);
        }
    }

    proptest! {
        #[test]
        fn all_frame_pair_round_trips_hold(
            lat in -90.0f64..90.0,
            lon in -720.0f64..720.0,
            geoid_offset_m in -120.0f64..120.0,
            ground_msl_m in -500.0f64..9000.0,
            value_m in -2000.0f64..50000.0,
            source in 0u8..3,
            mid in 0u8..3,
        ) {
            let geoid = MockGeoid::new(geoid_offset_m);
            let terrain = MockTerrain::new(ground_msl_m);
            let converter = AltitudeConverter::new(&geoid, &terrain);
            let point = GeoPoint::new(lat, lon).unwrap();

            let source_frame = match source {
                0 => VerticalFrame::Agl,
                1 => VerticalFrame::Msl,
                _ => VerticalFrame::Hae,
            };
            let mid_frame = match mid {
                0 => VerticalFrame::Agl,
                1 => VerticalFrame::Msl,
                _ => VerticalFrame::Hae,
            };

            let mid_sample = converter
                .convert_sample(
                    point,
                    AltitudeSample::new(value_m, source_frame).unwrap(),
                    mid_frame,
                )
                .unwrap();
            let back = converter
                .convert_sample(point, mid_sample, source_frame)
                .unwrap();

            prop_assert_eq!(back.frame, source_frame);
            prop_assert!((back.meters - value_m).abs() < 1e-9);
        }
    }
}
