use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::altitude::{AltitudeConverter, AltitudeError, GeoPoint, GeoidProvider, VerticalFrame};
use crate::egm96::{EGM2008, EGM96};
use crate::height::Interpolation;
use crate::terrain::{SrtmDataset, VoidPolicy};
use crate::wgs84::{AltType, Enu, Lla, Ned};

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn clear_last_error() {
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = CString::default();
    });
}

fn set_last_error(message: &str) {
    let sanitized = message.replace('\0', " ");
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = CString::new(sanitized).unwrap_or_default();
    });
}

fn fail(status: SwStatus, message: &str) -> SwStatus {
    set_last_error(message);
    status
}

fn to_interpolation(value: SwInterpolation) -> Result<Interpolation, SwStatus> {
    match value {
        SwInterpolation::Nearest => Ok(Interpolation::Nearest),
        SwInterpolation::Bilinear => Ok(Interpolation::Bilinear),
        SwInterpolation::Bicubic => Ok(Interpolation::Bicubic),
    }
}

fn to_vertical_frame(value: SwVerticalFrame) -> Result<VerticalFrame, SwStatus> {
    match value {
        SwVerticalFrame::Agl => Ok(VerticalFrame::Agl),
        SwVerticalFrame::Msl => Ok(VerticalFrame::Msl),
        SwVerticalFrame::Hae => Ok(VerticalFrame::Hae),
    }
}

fn to_void_policy(value: SwVoidPolicy, radius_cells: u32) -> Result<VoidPolicy, SwStatus> {
    match value {
        SwVoidPolicy::Error => Ok(VoidPolicy::Error),
        SwVoidPolicy::Zero => Ok(VoidPolicy::Zero),
        SwVoidPolicy::NearestValid => Ok(VoidPolicy::NearestValid {
            max_radius_cells: radius_cells as usize,
        }),
    }
}

fn to_path(ptr: *const c_char, name: &str) -> Result<PathBuf, SwStatus> {
    if ptr.is_null() {
        return Err(fail(
            SwStatus::NullPointer,
            &format!("{name} must be non-null"),
        ));
    }

    // SAFETY: caller guarantees `ptr` points to a valid NUL-terminated C string.
    let value = unsafe { CStr::from_ptr(ptr) };
    let text = value.to_str().map_err(|_| {
        fail(
            SwStatus::InvalidArgument,
            &format!("{name} must be valid UTF-8"),
        )
    })?;
    if text.is_empty() {
        return Err(fail(
            SwStatus::InvalidArgument,
            &format!("{name} must be non-empty"),
        ));
    }
    Ok(PathBuf::from(text))
}

fn point_from_components(lat_deg: f64, lon_deg: f64) -> Result<GeoPoint, SwStatus> {
    GeoPoint::new(lat_deg, lon_deg)
        .map_err(|err| fail(status_from_altitude_error(&err), &err.to_string()))
}

fn lla_from_sw(value: SwLlaWgs84) -> Result<Lla, SwStatus> {
    if !value.lat_deg.is_finite() || !(-90.0..=90.0).contains(&value.lat_deg) {
        return Err(fail(
            SwStatus::InvalidArgument,
            &format!(
                "lat_deg must be finite and within [-90, 90], got {}",
                value.lat_deg
            ),
        ));
    }
    if !value.lon_deg.is_finite() {
        return Err(fail(
            SwStatus::InvalidArgument,
            &format!("lon_deg must be finite, got {}", value.lon_deg),
        ));
    }
    if !value.hae_m.is_finite() {
        return Err(fail(
            SwStatus::InvalidArgument,
            &format!("hae_m must be finite, got {}", value.hae_m),
        ));
    }
    Ok(Lla::new(
        value.lat_deg,
        value.lon_deg,
        value.hae_m,
        AltType::Wgs84,
    ))
}

fn sw_from_lla(value: Lla) -> SwLlaWgs84 {
    SwLlaWgs84 {
        lat_deg: value.lat_deg(),
        lon_deg: value.lon_deg(),
        hae_m: value.alt_m(),
    }
}

fn status_from_altitude_error(err: &AltitudeError) -> SwStatus {
    match err {
        AltitudeError::InvalidCoordinate { .. } | AltitudeError::InvalidHeight { .. } => {
            SwStatus::InvalidArgument
        }
        AltitudeError::Geoid(_) | AltitudeError::Terrain(_) => SwStatus::QueryError,
    }
}

enum GeoidDataset {
    Egm96(EGM96),
    Egm2008(EGM2008),
}

impl GeoidProvider for GeoidDataset {
    fn geoid_offset_m(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        interpolation: Interpolation,
    ) -> Result<f64, AltitudeError> {
        match self {
            GeoidDataset::Egm96(grid) => match interpolation {
                Interpolation::Nearest => grid.offset(lat_deg, lon_deg),
                Interpolation::Bilinear => grid.offset_bilinear(lat_deg, lon_deg),
                Interpolation::Bicubic => grid.offset_bicubic(lat_deg, lon_deg),
            },
            GeoidDataset::Egm2008(grid) => match interpolation {
                Interpolation::Nearest => grid.offset(lat_deg, lon_deg),
                Interpolation::Bilinear => grid.offset_bilinear(lat_deg, lon_deg),
                Interpolation::Bicubic => grid.offset_bicubic(lat_deg, lon_deg),
            },
        }
        .map_err(AltitudeError::from)
    }
}

struct ConverterCore {
    geoid: GeoidDataset,
    terrain: SrtmDataset,
    geoid_interpolation: Interpolation,
    terrain_interpolation: Interpolation,
}

impl ConverterCore {
    fn altitude_converter(&self) -> AltitudeConverter<'_, GeoidDataset, SrtmDataset> {
        AltitudeConverter::new(&self.geoid, &self.terrain)
            .with_geoid_interpolation(self.geoid_interpolation)
            .with_terrain_interpolation(self.terrain_interpolation)
    }
}

/// Opaque converter handle for C/C++ callers.
pub struct SwConverterHandle {
    core: Mutex<ConverterCore>,
}

/// Status code returned by C ABI functions.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwStatus {
    Ok = 0,
    NullPointer = 1,
    InvalidArgument = 2,
    InitializationError = 3,
    QueryError = 4,
    InternalError = 5,
}

/// Geoid model selection.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwGeoidModel {
    Egm96 = 0,
    Egm2008 = 1,
}

/// Interpolation mode for geoid and terrain queries.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwInterpolation {
    Nearest = 0,
    Bilinear = 1,
    Bicubic = 2,
}

/// Vertical reference frame for altitude values.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwVerticalFrame {
    Agl = 0,
    Msl = 1,
    Hae = 2,
}

/// Terrain void sample handling policy.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwVoidPolicy {
    Error = 0,
    Zero = 1,
    NearestValid = 2,
}

/// Converter construction options.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SwConverterOptions {
    pub geoid_model: SwGeoidModel,
    pub geoid_interpolation: SwInterpolation,
    pub terrain_interpolation: SwInterpolation,
    pub terrain_cache_tiles: u32,
    pub void_policy: SwVoidPolicy,
    pub void_policy_radius_cells: u32,
    pub preload_geoid: u8,
}

/// Terrain/geoid reference terms for a geodetic location.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SwTerrainReference {
    pub geoid_offset_m: f64,
    pub ground_msl_m: f64,
    pub ground_hae_m: f64,
}

/// WGS84 geodetic point with HAE altitude in meters.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SwLlaWgs84 {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub hae_m: f64,
}

/// Local NED point in meters.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SwNed {
    pub n_m: f64,
    pub e_m: f64,
    pub d_m: f64,
}

/// Local ENU point in meters.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SwEnu {
    pub e_m: f64,
    pub n_m: f64,
    pub u_m: f64,
}

/// Returns default converter options.
///
/// # Safety
/// `out_options` must be non-null and point to writable memory.
#[no_mangle]
pub unsafe extern "C" fn sw_converter_options_default(
    out_options: *mut SwConverterOptions,
) -> SwStatus {
    if out_options.is_null() {
        return fail(SwStatus::NullPointer, "out_options must be non-null");
    }
    clear_last_error();

    // SAFETY: null pointer checked above, caller provides writable memory.
    unsafe {
        *out_options = SwConverterOptions {
            geoid_model: SwGeoidModel::Egm96,
            geoid_interpolation: SwInterpolation::Bilinear,
            terrain_interpolation: SwInterpolation::Bilinear,
            terrain_cache_tiles: 64,
            void_policy: SwVoidPolicy::Error,
            void_policy_radius_cells: 3,
            preload_geoid: 1,
        };
    }
    SwStatus::Ok
}

/// Creates a converter handle for altitude/frame queries from C/C++.
///
/// # Safety
/// - `geoid_path` and `terrain_root` must be valid NUL-terminated C strings.
/// - `options` may be null (defaults are used), otherwise must point to a valid options struct.
/// - `out_converter` must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn sw_converter_create(
    geoid_path: *const c_char,
    terrain_root: *const c_char,
    options: *const SwConverterOptions,
    out_converter: *mut *mut SwConverterHandle,
) -> SwStatus {
    if out_converter.is_null() {
        return fail(SwStatus::NullPointer, "out_converter must be non-null");
    }
    clear_last_error();

    let geoid_path = match to_path(geoid_path, "geoid_path") {
        Ok(path) => path,
        Err(status) => return status,
    };
    let terrain_root = match to_path(terrain_root, "terrain_root") {
        Ok(path) => path,
        Err(status) => return status,
    };

    let options_value = if options.is_null() {
        SwConverterOptions {
            geoid_model: SwGeoidModel::Egm96,
            geoid_interpolation: SwInterpolation::Bilinear,
            terrain_interpolation: SwInterpolation::Bilinear,
            terrain_cache_tiles: 64,
            void_policy: SwVoidPolicy::Error,
            void_policy_radius_cells: 3,
            preload_geoid: 1,
        }
    } else {
        // SAFETY: null checked above.
        unsafe { *options }
    };

    let geoid_interpolation = match to_interpolation(options_value.geoid_interpolation) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let terrain_interpolation = match to_interpolation(options_value.terrain_interpolation) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let void_policy = match to_void_policy(
        options_value.void_policy,
        options_value.void_policy_radius_cells,
    ) {
        Ok(value) => value,
        Err(status) => return status,
    };

    let geoid = match options_value.geoid_model {
        SwGeoidModel::Egm96 => {
            let mut grid = match EGM96::new(&geoid_path) {
                Ok(value) => value,
                Err(err) => {
                    return fail(
                        SwStatus::InitializationError,
                        &format!("failed to open EGM96 dataset: {err}"),
                    )
                }
            };
            if options_value.preload_geoid != 0 {
                if let Err(err) = grid.load_data() {
                    return fail(
                        SwStatus::InitializationError,
                        &format!("failed to preload EGM96 dataset: {err}"),
                    );
                }
            }
            GeoidDataset::Egm96(grid)
        }
        SwGeoidModel::Egm2008 => {
            let mut grid = match EGM2008::new(&geoid_path) {
                Ok(value) => value,
                Err(err) => {
                    return fail(
                        SwStatus::InitializationError,
                        &format!("failed to open EGM2008 dataset: {err}"),
                    )
                }
            };
            if options_value.preload_geoid != 0 {
                if let Err(err) = grid.load_data() {
                    return fail(
                        SwStatus::InitializationError,
                        &format!("failed to preload EGM2008 dataset: {err}"),
                    );
                }
            }
            GeoidDataset::Egm2008(grid)
        }
    };

    let terrain = SrtmDataset::new(terrain_root)
        .with_max_cached_tiles(options_value.terrain_cache_tiles as usize)
        .with_void_policy(void_policy);

    let handle = Box::new(SwConverterHandle {
        core: Mutex::new(ConverterCore {
            geoid,
            terrain,
            geoid_interpolation,
            terrain_interpolation,
        }),
    });

    // SAFETY: null checked above, caller provides writable memory.
    unsafe {
        *out_converter = Box::into_raw(handle);
    }
    SwStatus::Ok
}

/// Destroys a converter previously created with [`sw_converter_create`].
///
/// # Safety
/// `converter` must be either null or a pointer returned by `sw_converter_create`.
#[no_mangle]
pub unsafe extern "C" fn sw_converter_destroy(converter: *mut SwConverterHandle) {
    if converter.is_null() {
        return;
    }
    // SAFETY: pointer is expected to come from Box::into_raw in `sw_converter_create`.
    unsafe {
        drop(Box::from_raw(converter));
    }
}

/// Returns the calling thread's last error string from this C ABI.
///
/// The returned pointer remains valid until the next C ABI call on the same thread.
///
/// # Safety
/// Returned pointer must not be freed or mutated by the caller.
#[no_mangle]
pub unsafe extern "C" fn sw_last_error_message() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

/// Converts a scalar altitude value between explicit vertical frames.
///
/// # Safety
/// - `converter` must be a valid handle from `sw_converter_create`.
/// - `out_meters` must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn sw_converter_convert_height_m(
    converter: *const SwConverterHandle,
    lat_deg: f64,
    lon_deg: f64,
    meters: f64,
    source_frame: SwVerticalFrame,
    target_frame: SwVerticalFrame,
    out_meters: *mut f64,
) -> SwStatus {
    if converter.is_null() {
        return fail(SwStatus::NullPointer, "converter must be non-null");
    }
    if out_meters.is_null() {
        return fail(SwStatus::NullPointer, "out_meters must be non-null");
    }
    clear_last_error();

    let source_frame = match to_vertical_frame(source_frame) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let target_frame = match to_vertical_frame(target_frame) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let point = match point_from_components(lat_deg, lon_deg) {
        Ok(value) => value,
        Err(status) => return status,
    };

    // SAFETY: null checked above.
    let handle = unsafe { &*converter };
    let core = match handle.core.lock() {
        Ok(lock) => lock,
        Err(_) => return fail(SwStatus::InternalError, "converter lock poisoned"),
    };
    let converter = core.altitude_converter();
    let result = match converter.convert_height_m(point, meters, source_frame, target_frame) {
        Ok(value) => value,
        Err(err) => return fail(status_from_altitude_error(&err), &err.to_string()),
    };

    // SAFETY: null checked above.
    unsafe {
        *out_meters = result;
    }
    SwStatus::Ok
}

/// Returns terrain/geoid reference terms at a geodetic location.
///
/// # Safety
/// - `converter` must be a valid handle from `sw_converter_create`.
/// - `out_reference` must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn sw_converter_reference(
    converter: *const SwConverterHandle,
    lat_deg: f64,
    lon_deg: f64,
    out_reference: *mut SwTerrainReference,
) -> SwStatus {
    if converter.is_null() {
        return fail(SwStatus::NullPointer, "converter must be non-null");
    }
    if out_reference.is_null() {
        return fail(SwStatus::NullPointer, "out_reference must be non-null");
    }
    clear_last_error();

    let point = match point_from_components(lat_deg, lon_deg) {
        Ok(value) => value,
        Err(status) => return status,
    };

    // SAFETY: null checked above.
    let handle = unsafe { &*converter };
    let core = match handle.core.lock() {
        Ok(lock) => lock,
        Err(_) => return fail(SwStatus::InternalError, "converter lock poisoned"),
    };
    let converter = core.altitude_converter();
    let reference = match converter.reference_at(point) {
        Ok(value) => value,
        Err(err) => return fail(status_from_altitude_error(&err), &err.to_string()),
    };

    // SAFETY: null checked above.
    unsafe {
        *out_reference = SwTerrainReference {
            geoid_offset_m: reference.geoid_offset_m,
            ground_msl_m: reference.ground_msl_m,
            ground_hae_m: reference.ground_hae_m,
        };
    }
    SwStatus::Ok
}

/// Converts a geodetic point plus explicit source frame altitude into absolute WGS84/HAE LLA.
///
/// # Safety
/// - `converter` must be a valid handle from `sw_converter_create`.
/// - `out_lla` must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn sw_converter_lla_wgs84_from_height_m(
    converter: *const SwConverterHandle,
    lat_deg: f64,
    lon_deg: f64,
    meters: f64,
    source_frame: SwVerticalFrame,
    out_lla: *mut SwLlaWgs84,
) -> SwStatus {
    if converter.is_null() {
        return fail(SwStatus::NullPointer, "converter must be non-null");
    }
    if out_lla.is_null() {
        return fail(SwStatus::NullPointer, "out_lla must be non-null");
    }
    clear_last_error();

    let source_frame = match to_vertical_frame(source_frame) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let point = match point_from_components(lat_deg, lon_deg) {
        Ok(value) => value,
        Err(status) => return status,
    };

    // SAFETY: null checked above.
    let handle = unsafe { &*converter };
    let core = match handle.core.lock() {
        Ok(lock) => lock,
        Err(_) => return fail(SwStatus::InternalError, "converter lock poisoned"),
    };
    let converter = core.altitude_converter();
    let lla = match converter.lla_wgs84_from_height_m(point, meters, source_frame) {
        Ok(value) => value,
        Err(err) => return fail(status_from_altitude_error(&err), &err.to_string()),
    };

    // SAFETY: null checked above.
    unsafe {
        *out_lla = sw_from_lla(lla);
    }
    SwStatus::Ok
}

/// Returns terrain cache usage counters for a converter handle.
///
/// # Safety
/// - `converter` must be a valid handle from `sw_converter_create`.
/// - `out_cached_tiles` and `out_loaded_tiles` must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn sw_converter_terrain_cache_stats(
    converter: *const SwConverterHandle,
    out_cached_tiles: *mut u64,
    out_loaded_tiles: *mut u64,
) -> SwStatus {
    if converter.is_null() {
        return fail(SwStatus::NullPointer, "converter must be non-null");
    }
    if out_cached_tiles.is_null() {
        return fail(SwStatus::NullPointer, "out_cached_tiles must be non-null");
    }
    if out_loaded_tiles.is_null() {
        return fail(SwStatus::NullPointer, "out_loaded_tiles must be non-null");
    }
    clear_last_error();

    // SAFETY: null checked above.
    let handle = unsafe { &*converter };
    let core = match handle.core.lock() {
        Ok(lock) => lock,
        Err(_) => return fail(SwStatus::InternalError, "converter lock poisoned"),
    };

    // SAFETY: null checked above.
    unsafe {
        *out_cached_tiles = core.terrain.cached_tile_count() as u64;
        *out_loaded_tiles = core.terrain.loaded_tile_count() as u64;
    }
    SwStatus::Ok
}

/// Converts a local NED point to absolute WGS84/HAE LLA.
///
/// # Safety
/// `out_lla` must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn sw_wgs84_ned_to_lla(
    origin_lla_wgs84: SwLlaWgs84,
    point_ned_m: SwNed,
    out_lla: *mut SwLlaWgs84,
) -> SwStatus {
    if out_lla.is_null() {
        return fail(SwStatus::NullPointer, "out_lla must be non-null");
    }
    clear_last_error();

    let origin = match lla_from_sw(origin_lla_wgs84) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let lla = Ned::new(point_ned_m.n_m, point_ned_m.e_m, point_ned_m.d_m, origin).to_lla();

    // SAFETY: null checked above.
    unsafe {
        *out_lla = sw_from_lla(lla);
    }
    SwStatus::Ok
}

/// Converts an absolute WGS84/HAE LLA point to local NED at `origin_lla_wgs84`.
///
/// # Safety
/// `out_ned` must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn sw_wgs84_lla_to_ned(
    point_lla_wgs84: SwLlaWgs84,
    origin_lla_wgs84: SwLlaWgs84,
    out_ned: *mut SwNed,
) -> SwStatus {
    if out_ned.is_null() {
        return fail(SwStatus::NullPointer, "out_ned must be non-null");
    }
    clear_last_error();

    let point = match lla_from_sw(point_lla_wgs84) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let origin = match lla_from_sw(origin_lla_wgs84) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let ned = Ned::from_lla(point, origin);

    // SAFETY: null checked above.
    unsafe {
        *out_ned = SwNed {
            n_m: ned.n(),
            e_m: ned.e(),
            d_m: ned.d(),
        };
    }
    SwStatus::Ok
}

/// Converts an ENU point at one WGS84/HAE origin into NED at another WGS84/HAE origin.
///
/// # Safety
/// `out_ned` must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn sw_wgs84_enu_to_ned_between_origins(
    point_enu_m: SwEnu,
    enu_origin_lla_wgs84: SwLlaWgs84,
    ned_origin_lla_wgs84: SwLlaWgs84,
    out_ned: *mut SwNed,
) -> SwStatus {
    if out_ned.is_null() {
        return fail(SwStatus::NullPointer, "out_ned must be non-null");
    }
    clear_last_error();

    let enu_origin = match lla_from_sw(enu_origin_lla_wgs84) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let ned_origin = match lla_from_sw(ned_origin_lla_wgs84) {
        Ok(value) => value,
        Err(status) => return status,
    };

    let ned = Enu::new(
        point_enu_m.e_m,
        point_enu_m.n_m,
        point_enu_m.u_m,
        enu_origin,
    )
    .to_ned(ned_origin);

    // SAFETY: null checked above.
    unsafe {
        *out_ned = SwNed {
            n_m: ned.n(),
            e_m: ned.e(),
            d_m: ned.d(),
        };
    }
    SwStatus::Ok
}

/// Converts a local ENU point to absolute WGS84/HAE LLA.
///
/// # Safety
/// `out_lla` must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn sw_wgs84_enu_to_lla(
    point_enu_m: SwEnu,
    enu_origin_lla_wgs84: SwLlaWgs84,
    out_lla: *mut SwLlaWgs84,
) -> SwStatus {
    if out_lla.is_null() {
        return fail(SwStatus::NullPointer, "out_lla must be non-null");
    }
    clear_last_error();

    let enu_origin = match lla_from_sw(enu_origin_lla_wgs84) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let lla = Enu::new(
        point_enu_m.e_m,
        point_enu_m.n_m,
        point_enu_m.u_m,
        enu_origin,
    )
    .to_lla();

    // SAFETY: null checked above.
    unsafe {
        *out_lla = sw_from_lla(lla);
    }
    SwStatus::Ok
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        sw_converter_convert_height_m, sw_converter_create, sw_converter_destroy,
        sw_converter_lla_wgs84_from_height_m, sw_converter_options_default, sw_converter_reference,
        sw_converter_terrain_cache_stats, sw_last_error_message,
        sw_wgs84_enu_to_ned_between_origins, SwConverterHandle, SwConverterOptions, SwEnu,
        SwGeoidModel, SwInterpolation, SwLlaWgs84, SwNed, SwStatus, SwTerrainReference,
        SwVerticalFrame,
    };

    fn unique_temp_dir(label: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "small_world_ffi_{label}_{}_{}",
            std::process::id(),
            now
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_constant_egm96(path: &Path, offset_m: f64) -> PathBuf {
        let out = path.join("WW15MGH.DAC");
        let raw = (offset_m * 100.0).round() as i16;
        let mut bytes = Vec::with_capacity(721 * 1440 * 2);
        for _ in 0..(721 * 1440) {
            bytes.extend_from_slice(&raw.to_be_bytes());
        }
        fs::write(&out, bytes).unwrap();
        out
    }

    fn write_constant_hgt_tile(path: &Path, lat_floor_deg: i32, lon_floor_deg: i32, value_m: i16) {
        let lat_prefix = if lat_floor_deg >= 0 { 'N' } else { 'S' };
        let lon_prefix = if lon_floor_deg >= 0 { 'E' } else { 'W' };
        let filename = format!(
            "{lat_prefix}{:02}{lon_prefix}{:03}.hgt",
            lat_floor_deg.abs(),
            lon_floor_deg.abs()
        );
        let mut bytes = Vec::with_capacity(2 * 2 * 2);
        for _ in 0..4 {
            bytes.extend_from_slice(&value_m.to_be_bytes());
        }
        fs::write(path.join(filename), bytes).unwrap();
    }

    unsafe fn make_converter(root: &Path) -> *mut SwConverterHandle {
        let geoid_path = write_constant_egm96(root, 30.0);
        write_constant_hgt_tile(root, 0, 0, 120);
        let geoid_path = CString::new(geoid_path.to_string_lossy().as_bytes()).unwrap();
        let terrain_path = CString::new(root.to_string_lossy().as_bytes()).unwrap();

        let mut options = SwConverterOptions {
            geoid_model: SwGeoidModel::Egm96,
            geoid_interpolation: SwInterpolation::Bilinear,
            terrain_interpolation: SwInterpolation::Bilinear,
            terrain_cache_tiles: 64,
            void_policy: super::SwVoidPolicy::Error,
            void_policy_radius_cells: 3,
            preload_geoid: 1,
        };
        let status = sw_converter_options_default(&mut options);
        assert_eq!(status, SwStatus::Ok);

        let mut handle: *mut SwConverterHandle = std::ptr::null_mut();
        let status = sw_converter_create(
            geoid_path.as_ptr(),
            terrain_path.as_ptr(),
            &options,
            &mut handle,
        );
        assert_eq!(status, SwStatus::Ok);
        assert!(!handle.is_null());
        handle
    }

    #[test]
    fn ffi_height_conversion_and_lla_helpers_are_consistent() {
        let dir = unique_temp_dir("convert");
        // SAFETY: test inputs satisfy all API contracts.
        unsafe {
            let handle = make_converter(&dir);

            let mut hae_m = 0.0;
            let status = sw_converter_convert_height_m(
                handle,
                0.25,
                0.25,
                50.0,
                SwVerticalFrame::Agl,
                SwVerticalFrame::Hae,
                &mut hae_m,
            );
            assert_eq!(status, SwStatus::Ok);
            assert!((hae_m - 200.0).abs() < 1e-9);

            let mut lla = SwLlaWgs84 {
                lat_deg: 0.0,
                lon_deg: 0.0,
                hae_m: 0.0,
            };
            let status = sw_converter_lla_wgs84_from_height_m(
                handle,
                0.25,
                0.25,
                50.0,
                SwVerticalFrame::Agl,
                &mut lla,
            );
            assert_eq!(status, SwStatus::Ok);
            assert!((lla.lat_deg - 0.25).abs() < 1e-12);
            assert!((lla.lon_deg - 0.25).abs() < 1e-12);
            assert!((lla.hae_m - 200.0).abs() < 1e-9);

            let mut reference = SwTerrainReference {
                geoid_offset_m: 0.0,
                ground_msl_m: 0.0,
                ground_hae_m: 0.0,
            };
            let status = sw_converter_reference(handle, 0.25, 0.25, &mut reference);
            assert_eq!(status, SwStatus::Ok);
            assert!((reference.geoid_offset_m - 30.0).abs() < 1e-9);
            assert!((reference.ground_msl_m - 120.0).abs() < 1e-9);
            assert!((reference.ground_hae_m - 150.0).abs() < 1e-9);

            sw_converter_destroy(handle);
        }
    }

    #[test]
    fn ffi_wgs84_enu_to_ned_matches_axis_convention_for_same_origin() {
        // SAFETY: test inputs satisfy all API contracts.
        unsafe {
            let enu = SwEnu {
                e_m: 15.0,
                n_m: -4.0,
                u_m: 3.0,
            };
            let origin = SwLlaWgs84 {
                lat_deg: 39.0,
                lon_deg: -77.0,
                hae_m: 150.0,
            };
            let mut ned = SwNed {
                n_m: 0.0,
                e_m: 0.0,
                d_m: 0.0,
            };
            let status = sw_wgs84_enu_to_ned_between_origins(enu, origin, origin, &mut ned);
            assert_eq!(status, SwStatus::Ok);
            assert!((ned.n_m + 4.0).abs() < 1e-9);
            assert!((ned.e_m - 15.0).abs() < 1e-9);
            assert!((ned.d_m + 3.0).abs() < 1e-9);
        }
    }

    #[test]
    fn ffi_reports_invalid_arguments_via_status_and_last_error() {
        let dir = unique_temp_dir("errors");
        // SAFETY: test inputs satisfy all API contracts except deliberate invalid coordinate.
        unsafe {
            let handle = make_converter(&dir);

            let mut out = 0.0;
            let status = sw_converter_convert_height_m(
                handle,
                f64::NAN,
                0.0,
                10.0,
                SwVerticalFrame::Msl,
                SwVerticalFrame::Hae,
                &mut out,
            );
            assert_eq!(status, SwStatus::InvalidArgument);
            let message_ptr = sw_last_error_message();
            assert!(!message_ptr.is_null());
            let message = CStr::from_ptr(message_ptr).to_string_lossy().to_string();
            assert!(message.contains("lat_deg"));

            sw_converter_destroy(handle);
        }
    }

    #[test]
    fn ffi_repeated_queries_reuse_terrain_cache() {
        let dir = unique_temp_dir("cache");
        // SAFETY: test inputs satisfy all API contracts.
        unsafe {
            let handle = make_converter(&dir);

            for _ in 0..25_000 {
                let mut out = 0.0;
                let status = sw_converter_convert_height_m(
                    handle,
                    0.25,
                    0.25,
                    75.0,
                    SwVerticalFrame::Agl,
                    SwVerticalFrame::Hae,
                    &mut out,
                );
                assert_eq!(status, SwStatus::Ok);
                assert!((out - 225.0).abs() < 1e-9);
            }

            let mut cached_tiles = 0_u64;
            let mut loaded_tiles = 0_u64;
            let status =
                sw_converter_terrain_cache_stats(handle, &mut cached_tiles, &mut loaded_tiles);
            assert_eq!(status, SwStatus::Ok);
            assert_eq!(cached_tiles, 1);
            assert_eq!(loaded_tiles, 1);

            sw_converter_destroy(handle);
        }
    }
}
