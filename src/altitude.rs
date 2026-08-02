//! Altitude conversion primitives with explicit vertical frame semantics.
//!
//! This module intentionally requires explicit source/target frames for every conversion.
//! Use [`crate::altitude::AltitudeConverter::convert_height_m`] as the primary API for scalar
//! values.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::geoid::{EgmError, EGM2008, EGM96};

pub use crate::geoid::EgmModel;
use crate::height::Interpolation;
use crate::terrain::{SrtmDataset, TerrainError};
use crate::wgs84::{Ecef, Lla};

/// Vertical reference frame for altitude values in this crate.
///
/// All heights are in meters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerticalFrame {
    /// Height above local terrain surface from the DEM (`terrain_msl`).
    Agl,
    /// Orthometric height above mean sea level, **relative to the named geoid model**.
    ///
    /// "MSL" is not a single datum: EGM96 and EGM2008 undulations differ by decimeters over
    /// much of the Earth (locally approaching a meter), which dwarfs RTK-grade measurement
    /// noise. The model is therefore part of the frame, not a comment: every conversion checks
    /// the tag against the converter's geoid and fails with
    /// [`AltitudeError::GeoidModelMismatch`] instead of silently reinterpreting the value. If a
    /// data source does not document which model its MSL uses, that is a data problem to
    /// resolve — or avoid entirely by exchanging heights as `Hae`, which is model-free.
    Msl(EgmModel),
    /// Ellipsoidal height above the WGS84 reference ellipsoid (HAE).
    Hae,
}

impl Display for VerticalFrame {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VerticalFrame::Agl => f.write_str("AGL"),
            VerticalFrame::Msl(model) => write!(f, "MSL({model})"),
            VerticalFrame::Hae => f.write_str("HAE"),
        }
    }
}

/// Geodetic query location in decimal degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPoint {
    /// Geodetic latitude in decimal degrees, in `[-90, 90]`.
    pub lat_deg: f64,
    /// Geodetic longitude in decimal degrees.
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
    /// Height in meters, interpreted in `frame`.
    pub meters: f64,
    /// The vertical frame `meters` is expressed in.
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

    /// Convenience constructor for an `MSL` sample in meters, referenced to `model`.
    pub fn msl_m(meters: f64, model: EgmModel) -> Result<Self, AltitudeError> {
        Self::new(meters, VerticalFrame::Msl(model))
    }

    /// Convenience constructor for an `HAE` sample in meters.
    pub fn hae_m(meters: f64) -> Result<Self, AltitudeError> {
        Self::new(meters, VerticalFrame::Hae)
    }
}

impl Display for AltitudeSample {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.3} m {}", self.meters, self.frame)
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
    InvalidCoordinate {
        /// The parameter that failed validation (e.g. `"lat_deg"`).
        name: &'static str,
        /// The offending value.
        value: f64,
    },
    /// Invalid altitude/height argument.
    InvalidHeight {
        /// The parameter that failed validation (e.g. `"msl_m"`).
        name: &'static str,
        /// The offending value.
        value: f64,
    },
    /// An `Msl`-tagged value names a different geoid model than the converter's geoid.
    GeoidModelMismatch {
        /// The model named by the value being converted.
        value: EgmModel,
        /// The model of the geoid this converter was built with.
        converter: EgmModel,
    },
    /// A geoid re-reference ([`GeoidShift`]) was given a sample that is not `Msl` in its
    /// source model.
    ExpectedMslSample {
        /// The model the shift converts from.
        expected: EgmModel,
        /// The frame the sample actually carried.
        found: VerticalFrame,
    },
    /// The terrain dataset's orthometric heights are referenced to a different geoid model than
    /// the converter's geoid, so `Agl` conversions would mix datums.
    TerrainDatumMismatch {
        /// The geoid model the terrain heights are referenced to.
        terrain: EgmModel,
        /// The model of the geoid this converter was built with.
        geoid: EgmModel,
    },
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
            AltitudeError::GeoidModelMismatch { value, converter } => {
                write!(
                    f,
                    "MSL value is referenced to {value:?} but the converter's geoid is \
{converter:?}; re-reference it first with GeoidShift ({value:?} -> {converter:?}), or convert \
it with a {value:?} converter — the models differ by decimeters"
                )
            }
            AltitudeError::ExpectedMslSample { expected, found } => {
                write!(
                    f,
                    "geoid re-referencing needs an MSL sample referenced to {expected:?}, got \
{found:?}; convert to MSL in the source model first"
                )
            }
            AltitudeError::TerrainDatumMismatch { terrain, geoid } => {
                write!(
                    f,
                    "terrain heights are {terrain:?}-orthometric but the converter's geoid is \
{geoid:?}; AGL conversions would mix datums — use a {terrain:?} geoid with this terrain dataset"
                )
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
            AltitudeError::GeoidModelMismatch { .. } => None,
            AltitudeError::ExpectedMslSample { .. } => None,
            AltitudeError::TerrainDatumMismatch { .. } => None,
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

    /// The geoid model this provider implements. Defines what [`VerticalFrame::Msl`] means for
    /// any converter built on it, and is checked against every `Msl`-tagged value.
    fn model(&self) -> EgmModel;
}

macro_rules! forward_geoid_provider {
    ($($ptr:ty),+ $(,)?) => {$(
        impl<G: GeoidProvider + ?Sized> GeoidProvider for $ptr {
            fn geoid_offset_m(
                &self,
                lat_deg: f64,
                lon_deg: f64,
                interpolation: Interpolation,
            ) -> Result<f64, AltitudeError> {
                (**self).geoid_offset_m(lat_deg, lon_deg, interpolation)
            }

            fn model(&self) -> EgmModel {
                (**self).model()
            }
        }
    )+};
}
forward_geoid_provider!(&G, std::sync::Arc<G>, Box<G>);

/// Interface for terrain providers (`ground_msl` in `MSL = ground_msl + AGL`).
pub trait TerrainProvider {
    /// Returns terrain elevation in orthometric MSL meters at `(lat_deg, lon_deg)`.
    fn terrain_msl_m(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        interpolation: Interpolation,
    ) -> Result<f64, AltitudeError>;

    /// The geoid model this dataset's orthometric heights are referenced to, or `None` when the
    /// provider makes no orthometric claim (see [`NoTerrain`]). When `Some`, `Agl` conversions
    /// verify it against the converter's geoid so ground elevation and geoid separation are
    /// never mixed across datums.
    fn vertical_datum(&self) -> Option<EgmModel>;
}

macro_rules! forward_terrain_provider {
    ($($ptr:ty),+ $(,)?) => {$(
        impl<T: TerrainProvider + ?Sized> TerrainProvider for $ptr {
            fn terrain_msl_m(
                &self,
                lat_deg: f64,
                lon_deg: f64,
                interpolation: Interpolation,
            ) -> Result<f64, AltitudeError> {
                (**self).terrain_msl_m(lat_deg, lon_deg, interpolation)
            }

            fn vertical_datum(&self) -> Option<EgmModel> {
                (**self).vertical_datum()
            }
        }
    )+};
}
forward_terrain_provider!(&T, std::sync::Arc<T>, Box<T>);

/// The terrain provider for geoid-only converters ([`AltitudeConverter::geoid_only`]).
///
/// Makes no orthometric claim (`vertical_datum` is `None`) and fails every terrain query, so
/// `MSL <-> HAE` conversion works with zero terrain setup while any `Agl` conversion returns a
/// clear "no terrain dataset configured" error instead of a wrong answer.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoTerrain;

impl TerrainProvider for NoTerrain {
    fn terrain_msl_m(
        &self,
        _lat_deg: f64,
        _lon_deg: f64,
        _interpolation: Interpolation,
    ) -> Result<f64, AltitudeError> {
        Err(AltitudeError::Terrain(TerrainError::NotConfigured))
    }

    fn vertical_datum(&self) -> Option<EgmModel> {
        None
    }
}

impl GeoidProvider for EGM96 {
    fn model(&self) -> EgmModel {
        EgmModel::Egm96
    }

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
    fn model(&self) -> EgmModel {
        EgmModel::Egm2008
    }

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
    /// SRTM heights are published relative to the EGM96 geoid (NASA/USGS product definition).
    fn vertical_datum(&self) -> Option<EgmModel> {
        Some(EgmModel::Egm96)
    }

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

impl<G: GeoidProvider> AltitudeConverter<G, NoTerrain> {
    /// A converter for geoid-only work (`MSL <-> HAE`) that needs no terrain dataset at all.
    ///
    /// `Agl` conversions on this converter fail with a "no terrain dataset configured" error;
    /// everything else behaves identically to [`AltitudeConverter::new`].
    pub fn geoid_only(geoid: G) -> Self {
        Self::new(geoid, NoTerrain)
    }
}

/// Re-references orthometric heights between geoid models — the explicit bridge for systems
/// where one data source is EGM96-referenced and another EGM2008-referenced.
///
/// The ellipsoidal height is the invariant pivot: `MSL_to = MSL_from + (N_from − N_to)`, where
/// `N` is each model's undulation at the query point. The shift is decimeter-scale over most of
/// the Earth (locally approaching a meter), which is exactly the error silently absorbed when
/// mixed-source systems ignore the model.
///
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use small_world::altitude::{EgmModel, GeoPoint, GeoidShift};
/// use small_world::geoid::{EGM2008, EGM96};
///
/// let egm96 = EGM96::new(std::path::Path::new("data/WW15MGH.DAC"))?;
/// let egm2008 = EGM2008::new(std::path::Path::new("data/EGM2008_2_5.DAC"))?;
///
/// let to_2008 = GeoidShift::new(&egm96, &egm2008);
/// let p = GeoPoint::new(51.4779, -0.0015)?;
/// let msl_2008 = to_2008.convert_height_m(p, 46.0)?; // 46 m EGM96-MSL, re-referenced
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct GeoidShift<F, T>
where
    F: GeoidProvider,
    T: GeoidProvider,
{
    from: F,
    to: T,
    interpolation: Interpolation,
}

impl<F, T> GeoidShift<F, T>
where
    F: GeoidProvider,
    T: GeoidProvider,
{
    /// Creates a re-referencer from `from`'s model to `to`'s model.
    ///
    /// Equal models are permitted and act as an exact identity (no grid queries), so generic
    /// code need not special-case the degenerate direction.
    pub fn new(from: F, to: T) -> Self {
        Self {
            from,
            to,
            interpolation: Interpolation::Bilinear,
        }
    }

    /// Sets the interpolation used for both models' undulation lookups.
    pub fn with_interpolation(mut self, interpolation: Interpolation) -> Self {
        self.interpolation = interpolation;
        self
    }

    /// The model this shift converts from.
    pub fn from_model(&self) -> EgmModel {
        self.from.model()
    }

    /// The model this shift converts to.
    pub fn to_model(&self) -> EgmModel {
        self.to.model()
    }

    /// `N_from − N_to` at `point` in meters: the amount an MSL height grows when re-referenced.
    /// Exactly zero when the models are equal.
    pub fn shift_m(&self, point: GeoPoint) -> Result<f64, AltitudeError> {
        if self.from.model() == self.to.model() {
            return Ok(0.0);
        }
        let n_from = self
            .from
            .geoid_offset_m(point.lat_deg, point.lon_deg, self.interpolation)?;
        let n_to = self
            .to
            .geoid_offset_m(point.lat_deg, point.lon_deg, self.interpolation)?;
        Ok(n_from - n_to)
    }

    /// Re-references an MSL height in meters from the source model to the target model.
    pub fn convert_height_m(&self, point: GeoPoint, msl_m: f64) -> Result<f64, AltitudeError> {
        validate_height("msl_m", msl_m)?;
        Ok(msl_m + self.shift_m(point)?)
    }

    /// Typed variant: requires the sample to be `Msl` in the source model and returns it `Msl`
    /// in the target model, so the tag moves with the value.
    pub fn convert_sample(
        &self,
        point: GeoPoint,
        sample: AltitudeSample,
    ) -> Result<AltitudeSample, AltitudeError> {
        if sample.frame != VerticalFrame::Msl(self.from.model()) {
            return Err(AltitudeError::ExpectedMslSample {
                expected: self.from.model(),
                found: sample.frame,
            });
        }
        AltitudeSample::new(
            self.convert_height_m(point, sample.meters)?,
            VerticalFrame::Msl(self.to.model()),
        )
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

/// Frame-explicit altitude converter over a geoid provider and a terrain provider.
///
/// The geoid defines what [`VerticalFrame::Msl`] means for this converter; the terrain answers
/// `Agl` queries. Providers may be owned, borrowed, or shared (`&G`, `Arc<G>`, `Box<G>` all
/// implement the provider traits), so the converter can live in a long-lived service struct or
/// be built ad hoc from borrowed data. See [`AltitudeConverter::geoid_only`] when no terrain
/// dataset is involved.
#[derive(Clone, Copy, Debug)]
pub struct AltitudeConverter<G, T>
where
    G: GeoidProvider,
    T: TerrainProvider,
{
    geoid: G,
    terrain: T,
    geoid_interpolation: Interpolation,
    terrain_interpolation: Interpolation,
}

impl<G, T> AltitudeConverter<G, T>
where
    G: GeoidProvider,
    T: TerrainProvider,
{
    /// Creates a converter that combines:
    /// - geoid separation (`MSL <-> HAE`)
    /// - terrain elevation (`MSL <-> AGL`)
    ///
    /// The geoid you pass **defines what `Msl` means** for every conversion this converter
    /// performs (see [`VerticalFrame::Msl`]); the concrete models report which one they are via
    /// `EGM96::model()` / `EGM2008::model()`, so applications can record or assert provenance.
    pub fn new(geoid: G, terrain: T) -> Self {
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
    ///
    /// `ground_hae_m` sums terrain elevation and geoid separation, so this errors with
    /// [`AltitudeError::TerrainDatumMismatch`] if their datums differ.
    pub fn reference(&self, lat_deg: f64, lon_deg: f64) -> Result<TerrainReference, AltitudeError> {
        self.require_datum_coherence()?;
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
    ///
    /// Datum coherence is enforced, not assumed: an [`VerticalFrame::Msl`] tag naming a model
    /// other than the converter's geoid, or an [`VerticalFrame::Agl`] conversion over a terrain
    /// dataset referenced to a different model than the geoid, is an error — never a silent
    /// decimeter-scale reinterpretation.
    pub fn convert_sample(
        &self,
        point: GeoPoint,
        sample: AltitudeSample,
        target_frame: VerticalFrame,
    ) -> Result<AltitudeSample, AltitudeError> {
        if sample.frame == target_frame {
            return AltitudeSample::new(sample.meters, sample.frame);
        }
        for frame in [sample.frame, target_frame] {
            match frame {
                VerticalFrame::Msl(model) => self.require_geoid_model(model)?,
                VerticalFrame::Agl => self.require_datum_coherence()?,
                VerticalFrame::Hae => {}
            }
        }

        let msl_m = match sample.frame {
            VerticalFrame::Agl => self.msl_from_agl(point.lat_deg, point.lon_deg, sample.meters)?,
            VerticalFrame::Msl(_) => sample.meters,
            VerticalFrame::Hae => self.msl_from_hae(point.lat_deg, point.lon_deg, sample.meters)?,
        };

        let meters = match target_frame {
            VerticalFrame::Agl => self.agl_from_msl(point.lat_deg, point.lon_deg, msl_m)?,
            VerticalFrame::Msl(_) => msl_m,
            VerticalFrame::Hae => self.hae_from_msl(point.lat_deg, point.lon_deg, msl_m)?,
        };

        AltitudeSample::new(meters, target_frame)
    }

    /// The model that defines `Msl` for this converter: its geoid's.
    pub fn geoid_model(&self) -> EgmModel {
        self.geoid.model()
    }

    fn require_geoid_model(&self, value: EgmModel) -> Result<(), AltitudeError> {
        let converter = self.geoid.model();
        if value == converter {
            Ok(())
        } else {
            Err(AltitudeError::GeoidModelMismatch { value, converter })
        }
    }

    fn require_datum_coherence(&self) -> Result<(), AltitudeError> {
        // A provider with no orthometric claim (NoTerrain) has nothing to contradict; its
        // terrain queries fail on their own terms if an Agl conversion proceeds.
        let Some(terrain) = self.terrain.vertical_datum() else {
            return Ok(());
        };
        let geoid = self.geoid.model();
        if terrain == geoid {
            Ok(())
        } else {
            Err(AltitudeError::TerrainDatumMismatch { terrain, geoid })
        }
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
        Ok(Lla::new(point.lat_deg, point.lon_deg, hae_m))
    }

    /// Typed variant of [`Self::lla_wgs84_from_height_m`] using an [`AltitudeSample`].
    pub fn lla_wgs84_from_sample(
        &self,
        point: GeoPoint,
        sample: AltitudeSample,
    ) -> Result<Lla, AltitudeError> {
        let hae = self.convert_sample(point, sample, VerticalFrame::Hae)?;
        Ok(Lla::new(point.lat_deg, point.lon_deg, hae.meters))
    }

    /// Converts a height sample at `point` into absolute WGS84 ECEF coordinates in meters.
    pub fn ecef_wgs84_from_height_m(
        &self,
        point: GeoPoint,
        meters: f64,
        source_frame: VerticalFrame,
    ) -> Result<Ecef, AltitudeError> {
        let lla = self.lla_wgs84_from_height_m(point, meters, source_frame)?;
        Ok(lla.to_ecef())
    }

    /// Typed variant of [`Self::ecef_wgs84_from_height_m`] using an [`AltitudeSample`].
    pub fn ecef_wgs84_from_sample(
        &self,
        point: GeoPoint,
        sample: AltitudeSample,
    ) -> Result<Ecef, AltitudeError> {
        let lla = self.lla_wgs84_from_sample(point, sample)?;
        Ok(lla.to_ecef())
    }

    /// Converts an absolute WGS84 ECEF point into a scalar height in the explicit target frame.
    ///
    /// The ECEF point is interpreted as a WGS84 absolute position and converted via `HAE`.
    pub fn height_from_ecef_wgs84_m(
        &self,
        point_ecef_wgs84: Ecef,
        target_frame: VerticalFrame,
    ) -> Result<f64, AltitudeError> {
        let lla = Lla::from_ecef(point_ecef_wgs84);
        let point = GeoPoint::new(lla.lat_deg(), lla.lon_deg())?;
        self.convert_height_m(point, lla.alt_m(), VerticalFrame::Hae, target_frame)
    }

    /// Typed variant of [`Self::height_from_ecef_wgs84_m`] returning an [`AltitudeSample`].
    pub fn sample_from_ecef_wgs84(
        &self,
        point_ecef_wgs84: Ecef,
        target_frame: VerticalFrame,
    ) -> Result<AltitudeSample, AltitudeError> {
        let meters = self.height_from_ecef_wgs84_m(point_ecef_wgs84, target_frame)?;
        AltitudeSample::new(meters, target_frame)
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
        self.require_datum_coherence()?;
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
        self.require_datum_coherence()?;
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

    use super::{
        AltitudeConverter, AltitudeError, AltitudeSample, EgmModel, GeoPoint, GeoidProvider,
        GeoidShift, Interpolation, TerrainProvider, VerticalFrame,
    };

    struct MockGeoid {
        value_m: f64,
        model: EgmModel,
        interpolation_seen: Cell<Option<Interpolation>>,
        query_count: Cell<usize>,
    }

    impl MockGeoid {
        fn new(value_m: f64) -> Self {
            Self::with_model(value_m, EgmModel::Egm96)
        }

        fn with_model(value_m: f64, model: EgmModel) -> Self {
            Self {
                value_m,
                model,
                interpolation_seen: Cell::new(None),
                query_count: Cell::new(0),
            }
        }

        fn query_count(&self) -> usize {
            self.query_count.get()
        }
    }

    impl GeoidProvider for MockGeoid {
        fn model(&self) -> EgmModel {
            self.model
        }

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
        datum: EgmModel,
        interpolation_seen: Cell<Option<Interpolation>>,
        query_count: Cell<usize>,
    }

    impl MockTerrain {
        fn new(value_m: f64) -> Self {
            Self::with_datum(value_m, EgmModel::Egm96)
        }

        fn with_datum(value_m: f64, datum: EgmModel) -> Self {
            Self {
                value_m,
                datum,
                interpolation_seen: Cell::new(None),
                query_count: Cell::new(0),
            }
        }

        fn query_count(&self) -> usize {
            self.query_count.get()
        }
    }

    impl TerrainProvider for MockTerrain {
        fn vertical_datum(&self) -> Option<EgmModel> {
            Some(self.datum)
        }

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
    fn converters_work_owned_borrowed_and_shared() {
        // The ergonomics contract: the converter accepts its providers by value, by reference,
        // or in an Arc — so it can live in a long-lived service struct, be built ad hoc from
        // borrowed data, or share providers across threads, without wrapper types.
        struct GeoService {
            converter: AltitudeConverter<MockGeoid, MockTerrain>,
        }
        let service = GeoService {
            converter: AltitudeConverter::new(MockGeoid::new(30.0), MockTerrain::new(120.0)),
        };
        let point = GeoPoint::new(10.0, 20.0).unwrap();
        let hae = service
            .converter
            .convert_height_m(
                point,
                100.0,
                VerticalFrame::Msl(EgmModel::Egm96),
                VerticalFrame::Hae,
            )
            .unwrap();
        assert!((hae - 130.0).abs() < 1e-12);

        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let borrowed = AltitudeConverter::new(&geoid, &terrain);
        assert!(
            (borrowed
                .convert_height_m(
                    point,
                    100.0,
                    VerticalFrame::Msl(EgmModel::Egm96),
                    VerticalFrame::Hae
                )
                .unwrap()
                - hae)
                .abs()
                < 1e-12
        );

        // The Arc leg uses Sync providers (the interpolation-counting mocks hold Cells): a
        // shared converter is exactly the cross-thread case, so the providers must be Sync.
        struct ConstGeoid;
        impl GeoidProvider for ConstGeoid {
            fn geoid_offset_m(
                &self,
                _lat: f64,
                _lon: f64,
                _i: Interpolation,
            ) -> Result<f64, AltitudeError> {
                Ok(30.0)
            }
            fn model(&self) -> EgmModel {
                EgmModel::Egm96
            }
        }
        struct ConstTerrain;
        impl TerrainProvider for ConstTerrain {
            fn terrain_msl_m(
                &self,
                _lat: f64,
                _lon: f64,
                _i: Interpolation,
            ) -> Result<f64, AltitudeError> {
                Ok(120.0)
            }
            fn vertical_datum(&self) -> Option<EgmModel> {
                Some(EgmModel::Egm96)
            }
        }
        let shared = AltitudeConverter::new(
            std::sync::Arc::new(ConstGeoid),
            std::sync::Arc::new(ConstTerrain),
        );
        fn assert_send_sync<T: Send + Sync>(_: &T) {}
        assert_send_sync(&shared);
        assert!(
            (shared
                .convert_height_m(
                    point,
                    100.0,
                    VerticalFrame::Msl(EgmModel::Egm96),
                    VerticalFrame::Hae
                )
                .unwrap()
                - hae)
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn geoid_only_converter_needs_no_terrain() {
        // MSL <-> HAE with zero terrain setup; AGL fails with the "no terrain" error rather
        // than a datum error or a wrong answer.
        let converter = AltitudeConverter::geoid_only(MockGeoid::new(30.0));
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let hae = converter
            .convert_height_m(
                point,
                100.0,
                VerticalFrame::Msl(EgmModel::Egm96),
                VerticalFrame::Hae,
            )
            .unwrap();
        assert!((hae - 130.0).abs() < 1e-12);

        let err = converter
            .convert_height_m(point, 50.0, VerticalFrame::Agl, VerticalFrame::Hae)
            .unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("no terrain dataset configured"),
            "AGL on geoid_only must explain itself: {text}"
        );
    }

    #[test]
    fn display_formats_are_log_friendly() {
        assert_eq!(VerticalFrame::Hae.to_string(), "HAE");
        assert_eq!(
            VerticalFrame::Msl(EgmModel::Egm2008).to_string(),
            "MSL(EGM2008)"
        );
        assert_eq!(
            AltitudeSample::msl_m(46.0, EgmModel::Egm96)
                .unwrap()
                .to_string(),
            "46.000 m MSL(EGM96)"
        );
    }

    #[test]
    fn geoid_shift_rereferences_between_models() {
        // N96 = 30, N08 = 10 => an EGM96-MSL height gains N96 - N08 = +20 m when re-referenced
        // to EGM2008 (HAE is the invariant pivot).
        let egm96 = MockGeoid::with_model(30.0, EgmModel::Egm96);
        let egm2008 = MockGeoid::with_model(10.0, EgmModel::Egm2008);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let to_2008 = GeoidShift::new(&egm96, &egm2008);
        assert_eq!(to_2008.from_model(), EgmModel::Egm96);
        assert_eq!(to_2008.to_model(), EgmModel::Egm2008);
        assert!((to_2008.shift_m(point).unwrap() - 20.0).abs() < 1e-12);
        assert!((to_2008.convert_height_m(point, 100.0).unwrap() - 120.0).abs() < 1e-12);

        // Typed path: the tag moves with the value.
        let s96 = AltitudeSample::msl_m(100.0, EgmModel::Egm96).unwrap();
        let s08 = to_2008.convert_sample(point, s96).unwrap();
        assert_eq!(s08.frame, VerticalFrame::Msl(EgmModel::Egm2008));
        assert!((s08.meters - 120.0).abs() < 1e-12);

        // Round trip through the reverse shift is exact.
        let to_96 = GeoidShift::new(&egm2008, &egm96);
        let back = to_96.convert_sample(point, s08).unwrap();
        assert_eq!(back.frame, VerticalFrame::Msl(EgmModel::Egm96));
        assert!((back.meters - 100.0).abs() < 1e-12);

        // Mistagged and non-MSL samples are refused, with the fix in the message.
        let err = to_2008.convert_sample(point, s08).unwrap_err();
        assert!(matches!(
            err,
            AltitudeError::ExpectedMslSample {
                expected: EgmModel::Egm96,
                found: VerticalFrame::Msl(EgmModel::Egm2008),
            }
        ));
        let hae = AltitudeSample::hae_m(100.0).unwrap();
        assert!(to_2008.convert_sample(point, hae).is_err());
    }

    #[test]
    fn geoid_shift_between_equal_models_is_an_exact_identity() {
        // Equal models short-circuit: no undulation queries, bit-identical value out.
        let a = MockGeoid::with_model(30.0, EgmModel::Egm96);
        let b = MockGeoid::with_model(30.0, EgmModel::Egm96);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let shift = GeoidShift::new(&a, &b);
        assert_eq!(shift.shift_m(point).unwrap(), 0.0);
        assert_eq!(shift.convert_height_m(point, 123.456).unwrap(), 123.456);
        assert_eq!(a.query_count(), 0, "identity must not query the grids");
        assert_eq!(b.query_count(), 0);
    }

    #[test]
    fn geoid_shift_composes_coherently_with_converters() {
        // The property that makes the shift correct: HAE is invariant. Converting EGM96-MSL to
        // HAE directly must equal re-referencing to EGM2008-MSL first and converting via an
        // EGM2008 converter.
        let egm96 = MockGeoid::with_model(30.0, EgmModel::Egm96);
        let egm2008 = MockGeoid::with_model(10.0, EgmModel::Egm2008);
        let terrain96 = MockTerrain::new(120.0);
        let terrain08 = MockTerrain::with_datum(120.0, EgmModel::Egm2008);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let converter96 = AltitudeConverter::new(&egm96, &terrain96);
        let converter08 = AltitudeConverter::new(&egm2008, &terrain08);
        let to_2008 = GeoidShift::new(&egm96, &egm2008);

        let msl96 = 100.0;
        let hae_direct = converter96
            .convert_height_m(
                point,
                msl96,
                VerticalFrame::Msl(EgmModel::Egm96),
                VerticalFrame::Hae,
            )
            .unwrap();
        let msl08 = to_2008.convert_height_m(point, msl96).unwrap();
        let hae_via_shift = converter08
            .convert_height_m(
                point,
                msl08,
                VerticalFrame::Msl(EgmModel::Egm2008),
                VerticalFrame::Hae,
            )
            .unwrap();
        assert!(
            (hae_direct - hae_via_shift).abs() < 1e-12,
            "{hae_direct} vs {hae_via_shift}"
        );
    }

    #[test]
    fn mismatched_msl_model_is_rejected_not_reinterpreted() {
        // The failure this exists for: an EGM2008-referenced MSL value fed to an EGM96
        // converter differs by decimeters — silently converting it would bury a systematic
        // error 10-100x RTK measurement noise. It must refuse, in both directions.
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let err = converter
            .convert_height_m(
                point,
                100.0,
                VerticalFrame::Msl(EgmModel::Egm2008),
                VerticalFrame::Hae,
            )
            .unwrap_err();
        assert!(
            matches!(
                err,
                AltitudeError::GeoidModelMismatch {
                    value: EgmModel::Egm2008,
                    converter: EgmModel::Egm96,
                }
            ),
            "got {err:?}"
        );

        let err = converter
            .convert_height_m(
                point,
                100.0,
                VerticalFrame::Hae,
                VerticalFrame::Msl(EgmModel::Egm2008),
            )
            .unwrap_err();
        assert!(matches!(err, AltitudeError::GeoidModelMismatch { .. }));

        // The message must say what to do, not merely that it failed.
        let text = err.to_string();
        assert!(text.contains("Egm2008") && text.contains("Egm96"), "{text}");

        // A correctly tagged value converts exactly as before.
        let hae = converter
            .convert_height_m(
                point,
                100.0,
                VerticalFrame::Msl(EgmModel::Egm96),
                VerticalFrame::Hae,
            )
            .unwrap();
        assert!((hae - 130.0).abs() < 1e-12);
    }

    #[test]
    fn agl_refuses_to_mix_terrain_and_geoid_datums() {
        // SRTM-class DEMs are EGM96-orthometric. Pairing one with an EGM2008 geoid makes every
        // AGL<->HAE conversion sum heights from two different datums; that must be an error,
        // not an answer. HAE<->MSL stays available — it never touches the terrain.
        let geoid = MockGeoid::with_model(30.0, EgmModel::Egm2008);
        let terrain = MockTerrain::new(120.0); // EGM96-referenced
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let err = converter
            .convert_height_m(point, 50.0, VerticalFrame::Agl, VerticalFrame::Hae)
            .unwrap_err();
        assert!(
            matches!(
                err,
                AltitudeError::TerrainDatumMismatch {
                    terrain: EgmModel::Egm96,
                    geoid: EgmModel::Egm2008,
                }
            ),
            "got {err:?}"
        );
        assert!(converter.agl_from_hae(10.0, 20.0, 200.0).is_err());
        assert!(converter.reference(10.0, 20.0).is_err());

        // Geoid-only conversions remain valid with this pairing.
        let msl = converter
            .convert_height_m(
                point,
                130.0,
                VerticalFrame::Hae,
                VerticalFrame::Msl(EgmModel::Egm2008),
            )
            .unwrap();
        assert!((msl - 100.0).abs() < 1e-12);

        // And a coherent EGM2008 pairing does full AGL conversions.
        let terrain_08 = MockTerrain::with_datum(120.0, EgmModel::Egm2008);
        let converter_08 = AltitudeConverter::new(&geoid, &terrain_08);
        let hae = converter_08
            .convert_height_m(point, 50.0, VerticalFrame::Agl, VerticalFrame::Hae)
            .unwrap();
        assert!((hae - 200.0).abs() < 1e-12);
    }

    #[test]
    fn typed_conversion_matrix_is_consistent() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let agl = AltitudeSample::agl_m(50.0).unwrap();
        let msl = converter
            .convert_sample(point, agl, VerticalFrame::Msl(EgmModel::Egm96))
            .unwrap();
        let hae = converter
            .convert_sample(point, agl, VerticalFrame::Hae)
            .unwrap();

        assert_eq!(msl.frame, VerticalFrame::Msl(EgmModel::Egm96));
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
    }

    #[test]
    fn ecef_wgs84_from_height_matches_lla_helper() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let from_lla = converter
            .lla_wgs84_from_height_m(point, 50.0, VerticalFrame::Agl)
            .unwrap()
            .to_ecef();
        let from_ecef = converter
            .ecef_wgs84_from_height_m(point, 50.0, VerticalFrame::Agl)
            .unwrap();

        assert!((from_lla.x() - from_ecef.x()).abs() < 1e-9);
        assert!((from_lla.y() - from_ecef.y()).abs() < 1e-9);
        assert!((from_lla.z() - from_ecef.z()).abs() < 1e-9);
    }

    #[test]
    fn ecef_typed_helpers_are_consistent_with_scalar_variants() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();
        let sample = AltitudeSample::agl_m(50.0).unwrap();

        let from_scalar = converter
            .ecef_wgs84_from_height_m(point, 50.0, VerticalFrame::Agl)
            .unwrap();
        let from_sample = converter.ecef_wgs84_from_sample(point, sample).unwrap();

        assert!((from_scalar.x() - from_sample.x()).abs() < 1e-9);
        assert!((from_scalar.y() - from_sample.y()).abs() < 1e-9);
        assert!((from_scalar.z() - from_sample.z()).abs() < 1e-9);
    }

    #[test]
    fn ecef_to_frame_height_round_trip_is_consistent() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let frames = [
            VerticalFrame::Agl,
            VerticalFrame::Msl(EgmModel::Egm96),
            VerticalFrame::Hae,
        ];
        for frame in frames {
            let source_m = 250.0;
            let point_ecef = converter
                .ecef_wgs84_from_height_m(point, source_m, frame)
                .unwrap();
            let recovered = converter
                .height_from_ecef_wgs84_m(point_ecef, frame)
                .unwrap();
            assert!(
                (recovered - source_m).abs() < 1e-9,
                "frame={frame:?} recovered={recovered} source={source_m}"
            );
        }
    }

    #[test]
    fn sample_from_ecef_wgs84_sets_explicit_target_frame() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let point_ecef = converter
            .ecef_wgs84_from_height_m(point, 50.0, VerticalFrame::Agl)
            .unwrap();
        let sample = converter
            .sample_from_ecef_wgs84(point_ecef, VerticalFrame::Msl(EgmModel::Egm96))
            .unwrap();
        let expected_msl = converter
            .convert_height_m(
                point,
                50.0,
                VerticalFrame::Agl,
                VerticalFrame::Msl(EgmModel::Egm96),
            )
            .unwrap();

        assert_eq!(sample.frame, VerticalFrame::Msl(EgmModel::Egm96));
        assert!((sample.meters - expected_msl).abs() < 1e-9);
    }

    #[test]
    fn same_frame_conversion_is_strict_identity_and_query_free() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let frames = [
            VerticalFrame::Agl,
            VerticalFrame::Msl(EgmModel::Egm96),
            VerticalFrame::Hae,
        ];
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
        assert_eq!(geoid.query_count(), 0);
        assert_eq!(terrain.query_count(), 0);
    }

    #[test]
    fn conversion_matrix_matches_closed_form_relationships() {
        let geoid = MockGeoid::new(30.0);
        let terrain = MockTerrain::new(120.0);
        let converter = AltitudeConverter::new(&geoid, &terrain);
        let point = GeoPoint::new(10.0, 20.0).unwrap();

        let frames = [
            VerticalFrame::Agl,
            VerticalFrame::Msl(EgmModel::Egm96),
            VerticalFrame::Hae,
        ];
        for source in frames {
            for target in frames {
                let input = 250.0;
                let expected_msl = match source {
                    VerticalFrame::Agl => input + 120.0,
                    VerticalFrame::Msl(_) => input,
                    VerticalFrame::Hae => input - 30.0,
                };
                let expected = match target {
                    VerticalFrame::Agl => expected_msl - 120.0,
                    VerticalFrame::Msl(_) => expected_msl,
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
            (
                VerticalFrame::Agl,
                VerticalFrame::Msl(EgmModel::Egm96),
                0,
                1,
            ),
            (VerticalFrame::Agl, VerticalFrame::Hae, 1, 1),
            (
                VerticalFrame::Msl(EgmModel::Egm96),
                VerticalFrame::Agl,
                0,
                1,
            ),
            (
                VerticalFrame::Msl(EgmModel::Egm96),
                VerticalFrame::Msl(EgmModel::Egm96),
                0,
                0,
            ),
            (
                VerticalFrame::Msl(EgmModel::Egm96),
                VerticalFrame::Hae,
                1,
                0,
            ),
            (VerticalFrame::Hae, VerticalFrame::Agl, 1, 1),
            (
                VerticalFrame::Hae,
                VerticalFrame::Msl(EgmModel::Egm96),
                1,
                0,
            ),
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
            frame: VerticalFrame::Msl(EgmModel::Egm96),
        };
        let err = converter
            .convert_sample(point, invalid_sample, VerticalFrame::Msl(EgmModel::Egm96))
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
            let msl = converter.convert_sample(point, agl, VerticalFrame::Msl(EgmModel::Egm96)).unwrap();
            let hae = converter.convert_sample(point, msl, VerticalFrame::Hae).unwrap();
            let agl_back = converter.convert_sample(point, hae, VerticalFrame::Agl).unwrap();

            prop_assert_eq!(msl.frame, VerticalFrame::Msl(EgmModel::Egm96));
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
                1 => VerticalFrame::Msl(EgmModel::Egm96),
                _ => VerticalFrame::Hae,
            };
            let target_frame = match target {
                0 => VerticalFrame::Agl,
                1 => VerticalFrame::Msl(EgmModel::Egm96),
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
                1 => VerticalFrame::Msl(EgmModel::Egm96),
                _ => VerticalFrame::Hae,
            };
            let mid_frame = match mid {
                0 => VerticalFrame::Agl,
                1 => VerticalFrame::Msl(EgmModel::Egm96),
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

    proptest! {
        #[test]
        fn ecef_round_trip_holds_for_all_vertical_frames(
            lat in -89.0f64..89.0,
            lon in -720.0f64..720.0,
            geoid_offset_m in -120.0f64..120.0,
            ground_msl_m in -500.0f64..9000.0,
            value_m in -2000.0f64..50000.0,
            frame_idx in 0u8..3,
        ) {
            let geoid = MockGeoid::new(geoid_offset_m);
            let terrain = MockTerrain::new(ground_msl_m);
            let converter = AltitudeConverter::new(&geoid, &terrain);
            let point = GeoPoint::new(lat, lon).unwrap();

            let frame = match frame_idx {
                0 => VerticalFrame::Agl,
                1 => VerticalFrame::Msl(EgmModel::Egm96),
                _ => VerticalFrame::Hae,
            };

            let point_ecef = converter
                .ecef_wgs84_from_height_m(point, value_m, frame)
                .unwrap();
            let round_trip = converter
                .height_from_ecef_wgs84_m(point_ecef, frame)
                .unwrap();

            prop_assert!((round_trip - value_m).abs() < 2e-4);
        }
    }
}
