//! `small_world` provides explicit, frame-safe geodesy utilities for robotics/autonomy.
//!
//! The public API is organized around two primary workflows:
//! - altitude reference conversion (`AGL`/`MSL`/`HAE`) via [`altitude::AltitudeConverter`]
//! - local/global frame conversion (`LLA`/`ECEF`/`NED`/`ENU`) via [`wgs84`]
//!
//! # Frame contract
//! - `AGL`: meters above local terrain surface.
//! - `MSL`: orthometric meters above geoid (mean sea level).
//! - `HAE`: ellipsoidal meters above WGS84 ellipsoid.
//! - `NED.d`: positive down.
//! - `ENU.u`: positive up.
//!
//! # Minimal altitude conversion example
//! ```no_run
//! use std::path::Path;
//!
//! use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
//! use small_world::egm96::EGM96;
//! use small_world::terrain::SrtmDataset;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let geoid = EGM96::new(Path::new("data/WW15MGH.DAC"))?;
//!     let terrain = SrtmDataset::new("data/srtm");
//!     let converter = AltitudeConverter::new(&geoid, &terrain);
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
//! use small_world::wgs84::{AltType, Enu, Lla, Ned};
//!
//! let origin = Lla::try_new(39.0, -77.0, 150.0, AltType::Wgs84).unwrap();
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
//! See `Readme.md` for quick-start usage and `docs/PRODUCTION.md` for validation/deployment details.

/// Frame-explicit altitude conversion API (`AGL`/`MSL`/`HAE`).
pub mod altitude;
/// EGM96/EGM2008 geoid grid readers and interpolation helpers.
pub mod egm96;
/// Stable C ABI for C/C++ integration.
pub mod ffi;
/// Interpolation mode enum used by terrain/geoid queries.
pub mod height;
/// Low-level interpolation kernels.
pub mod interpolate;
/// SRTM terrain dataset loading, interpolation, and cache policy.
pub mod terrain;
/// WGS84 local/global frame transforms (`LLA`/`ECEF`/`NED`/`ENU`).
pub mod wgs84;
