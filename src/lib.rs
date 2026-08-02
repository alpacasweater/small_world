//! `small_world` provides explicit, frame-safe geodesy utilities for robotics/autonomy.
//!
//! The public API is organized around two primary workflows:
//! - altitude reference conversion (`AGL`/`MSL`/`HAE`) via [`altitude::AltitudeConverter`]
//! - local/global frame conversion (`LLA`/`ECEF`/`NED`/`ENU`) via [`wgs84`]
//!
//! # Frame contract
//! - `AGL`: meters above local terrain surface.
//! - `MSL`: orthometric meters above a **named** geoid model (`Msl(EgmModel)`); model
//!   mismatches are errors, never silent reinterpretation.
//! - `HAE`: ellipsoidal meters above WGS84 ellipsoid.
//! - `NED.d`: positive down.
//! - `ENU.u`: positive up.
//!
//! # Minimal altitude conversion example
//! ```no_run
//! use small_world::prelude::*;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let converter = AltitudeConverter::new(
//!         EGM96::new("data/WW15MGH.DAC")?, // or EGM96::embedded()? with `embedded-egm96`
//!         SrtmDataset::new("data/srtm"),
//!     );
//!
//!     let point = GeoPoint::new(39.0, -77.0)?;
//!     let alt_hae_m = converter.convert_height_m(
//!         point,
//!         120.0,
//!         VerticalFrame::Agl,
//!         VerticalFrame::Hae,
//!     )?;
//!     println!("AGL -> HAE: {alt_hae_m:.3} m");
//!     Ok(())
//! }
//! ```
//!
//! # Minimal local-frame conversion example
//! ```rust
//! use small_world::wgs84::{Enu, Lla, Ned};
//!
//! let origin = Lla::try_new(39.0, -77.0, 150.0).unwrap();
//! let enu = Enu::new(10.0, 5.0, 2.0, origin);
//! let ned_at_same_origin: Ned = enu.to_ned(origin);
//! assert!((ned_at_same_origin.n() - 5.0).abs() < 1e-6);
//! assert!((ned_at_same_origin.e() - 10.0).abs() < 1e-6);
//! assert!((ned_at_same_origin.d() + 2.0).abs() < 1e-6);
//! let ecef = origin.to_ecef();
//! let origin_back = Lla::from_ecef(ecef);
//! assert!((origin_back.alt_m() - origin.alt_m()).abs() < 1e-4);
//! ```
//!
//! Use `try_new` constructors when validating external input. The `new` constructors in
//! [`wgs84`] remain available as unchecked building blocks.
//!
//! See `README.md` for quick-start usage and `docs/PRODUCTION.md` for validation/deployment details.

#![warn(missing_docs)]

/// Frame-explicit altitude conversion API (`AGL`/`MSL`/`HAE`).
pub mod altitude;
/// Stable C ABI for C/C++ integration.
pub mod ffi;
/// EGM96/EGM2008 geoid grid readers and interpolation helpers.
pub mod geoid;
/// Interpolation mode enum used by terrain/geoid queries.
pub mod height;
/// Low-level interpolation kernels.
pub mod interpolate;
/// SRTM terrain dataset loading, interpolation, and cache policy.
pub mod terrain;
/// WGS84 local/global frame transforms (`LLA`/`ECEF`/`NED`/`ENU`).
pub mod wgs84;

/// One-line import of the everyday API surface.
///
/// ```
/// use small_world::prelude::*;
/// ```
///
/// Brings in the converters ([`AltitudeConverter`](crate::altitude::AltitudeConverter),
/// [`GeoidShift`](crate::altitude::GeoidShift)), the frame and sample types
/// ([`VerticalFrame`](crate::altitude::VerticalFrame), [`EgmModel`](crate::geoid::EgmModel),
/// [`AltitudeSample`](crate::altitude::AltitudeSample),
/// [`GeoPoint`](crate::altitude::GeoPoint)), the data providers
/// ([`EGM96`](crate::geoid::EGM96), [`EGM2008`](crate::geoid::EGM2008),
/// [`SrtmDataset`](crate::terrain::SrtmDataset), [`NoTerrain`](crate::altitude::NoTerrain),
/// the provider traits, and [`Interpolation`](crate::height::Interpolation)), and the
/// coordinate types ([`Lla`](crate::wgs84::Lla), [`Ecef`](crate::wgs84::Ecef),
/// [`Ned`](crate::wgs84::Ned), [`Enu`](crate::wgs84::Enu)).
pub mod prelude {
    pub use crate::altitude::{
        AltitudeConverter, AltitudeError, AltitudeSample, EgmModel, GeoPoint, GeoidProvider,
        GeoidShift, NoTerrain, TerrainProvider, VerticalFrame,
    };
    pub use crate::geoid::{EGM2008, EGM96};
    pub use crate::height::Interpolation;
    pub use crate::terrain::SrtmDataset;
    pub use crate::wgs84::{Ecef, Enu, Lla, Ned};
}
