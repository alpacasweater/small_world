//! EGM geoid grid readers and interpolation for EGM96/EGM2008 datasets.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};

use crate::interpolate::bicubic_unit;

const EGM96_ROWS: usize = 721;
const EGM96_COLS: usize = 1440;
const EGM2008_ROWS: usize = 4321;
const EGM2008_COLS: usize = 8640;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GridFormat {
    Egm96I16Be,
    Egm2008F32LeFortranSequential,
}

/// Identifies which Earth Gravitational Model a geoid grid was derived from.
///
/// The model determines the grid's resolution and on-disk encoding, and serves as provenance
/// for any mean-sea-level height computed through it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EgmModel {
    /// The NGA EGM96 15-arc-minute worldwide geoid grid (`WW15MGH.DAC`), stored as big-endian
    /// `i16` values in centimeters.
    Egm96,
    /// The NGA EGM2008 2.5-arc-minute worldwide geoid grid (`EGM2008_2_5.DAC`), stored as
    /// little-endian `f32` values in meters framed by Fortran sequential record markers.
    Egm2008,
}

impl Display for EgmModel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            EgmModel::Egm96 => "EGM96",
            EgmModel::Egm2008 => "EGM2008",
        })
    }
}

impl EgmModel {
    /// The official NGA download URL for this model's interpolation grid, as published on
    /// earth-info.nga.mil.
    pub fn dataset_url(self) -> &'static str {
        match self {
            EgmModel::Egm96 => {
                "https://earth-info.nga.mil/php/download.php?file=egm-96interpolation"
            }
            EgmModel::Egm2008 => {
                "https://earth-info.nga.mil/php/download.php?file=egm-08interpolation"
            }
        }
    }

    /// The filename the model's grid is published under by NGA (`WW15MGH.DAC` for EGM96,
    /// `EGM2008_2_5.DAC` for EGM2008), used as the expected name when staging data locally.
    pub fn canonical_filename(self) -> &'static str {
        match self {
            EgmModel::Egm96 => "WW15MGH.DAC",
            EgmModel::Egm2008 => "EGM2008_2_5.DAC",
        }
    }

    /// A copy-pasteable shell command that fetches and stages this model's grid under `data/`,
    /// without needing a checkout of this repository. Surfaced by [`EgmError::DatasetMissing`] so
    /// a missing dataset explains its own fix.
    pub fn download_command(self) -> String {
        let model = match self {
            EgmModel::Egm96 => "egm96",
            EgmModel::Egm2008 => "egm2008",
        };
        format!(
            "curl -fsSL https://raw.githubusercontent.com/alpacasweater/small_world/main/\
scripts/download_geoid_data.sh | bash -s -- --model {model}"
        )
    }
}

/// Errors raised while opening, parsing, or querying an EGM geoid grid.
#[derive(Debug)]
pub enum EgmError {
    /// An underlying filesystem read or seek on the grid file failed.
    Io(io::Error),
    /// The requested latitude was not finite or fell outside [-90, 90] degrees; the payload is
    /// the offending value in degrees.
    InvalidLatitude(f64),
    /// The requested longitude was not finite; the payload is the offending value in degrees.
    /// (Finite longitudes of any magnitude are accepted and wrapped into [0, 360).)
    InvalidLongitude(f64),
    /// A direct grid lookup addressed a cell outside the grid's dimensions.
    InvalidIndex {
        /// The requested row index (0 at 90° N latitude, increasing southward).
        row: usize,
        /// The requested column index (0 at 0° E longitude, increasing eastward).
        col: usize,
        /// The largest valid row index for this grid.
        max_row: usize,
        /// The largest valid column index for this grid.
        max_col: usize,
    },
    /// The dataset's byte length does not match the model's known grid layout, so the file is
    /// truncated, corrupted, or not the expected NGA product.
    InvalidGridSize {
        /// The model whose layout the data was checked against.
        model: EgmModel,
        /// The byte length the model's grid layout requires.
        expected_bytes: usize,
        /// The byte length actually found on disk (or implied by a corrupt record marker).
        actual_bytes: u64,
    },
    /// The grid file does not exist. Carries everything needed to fix it: the Display message
    /// includes the exact download command (and, for EGM96, the embedded-data alternative).
    DatasetMissing {
        /// The model whose grid was being opened.
        model: EgmModel,
        /// The path that was tried and found absent.
        path: PathBuf,
    },
}

impl Display for EgmError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EgmError::Io(err) => write!(f, "I/O error: {err}"),
            EgmError::InvalidLatitude(lat) => {
                write!(f, "latitude must be finite and within [-90, 90], got {lat}")
            }
            EgmError::InvalidLongitude(lon) => {
                write!(f, "longitude must be finite, got {lon}")
            }
            EgmError::InvalidIndex {
                row,
                col,
                max_row,
                max_col,
            } => {
                write!(
                    f,
                    "grid index out of bounds: row={row}, col={col}, max_row={max_row}, max_col={max_col}"
                )
            }
            EgmError::InvalidGridSize {
                model,
                expected_bytes,
                actual_bytes,
            } => {
                write!(
                    f,
                    "unexpected {:?} grid size, expected {} bytes, got {} bytes",
                    model, expected_bytes, actual_bytes
                )
            }
            EgmError::DatasetMissing { model, path } => {
                write!(
                    f,
                    "{:?} geoid grid not found at {} — fetch it with: {}",
                    model,
                    path.display(),
                    model.download_command()
                )?;
                if *model == EgmModel::Egm96 {
                    write!(
                        f,
                        " (or skip the download: enable the `embedded-egm96` feature and use \
EGM96::embedded())"
                    )?;
                }
                Ok(())
            }
        }
    }
}

impl Error for EgmError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            EgmError::Io(err) => Some(err),
            EgmError::InvalidLatitude(_) => None,
            EgmError::InvalidLongitude(_) => None,
            EgmError::InvalidIndex { .. } => None,
            EgmError::InvalidGridSize { .. } => None,
            EgmError::DatasetMissing { .. } => None,
        }
    }
}

impl From<io::Error> for EgmError {
    fn from(value: io::Error) -> Self {
        EgmError::Io(value)
    }
}

/// A model-agnostic EGM geoid grid: a regular latitude/longitude raster of geoid undulations
/// N in meters (HAE = MSL + N), read either lazily from the backing dataset file or entirely
/// from memory.
///
/// Rows run from 90° N (row 0) southward to 90° S; columns run eastward from 0° E, wrapping at
/// 360°. [`EGM96`] and [`EGM2008`] are thin model-specific wrappers around this type.
#[derive(Debug)]
pub struct EgmGrid {
    // `None` once the grid has been fully materialized in memory (e.g. via `from_bytes`); in
    // that mode `read_geoid_value` never touches the filesystem. A `Mutex` (not `RefCell`) so
    // the grid types are `Sync` and can be shared across threads or stored in ECS resources;
    // lock poisoning is ignored because the guarded state is only a seek cursor.
    data_file: Option<Mutex<File>>,
    geoid: Vec<f64>,
    model: EgmModel,
    format: GridFormat,
    rows: usize,
    cols_storage: usize,
    lon_bins: usize,
    lat_step_deg: f64,
    lon_step_deg: f64,
}

impl EgmGrid {
    /// Opens the dataset file at `path` as a grid for `model`, validating its size against the
    /// model's known layout but reading no values yet: queries seek into the file on demand
    /// until [`load_data`](Self::load_data) materializes the grid in memory. Fails with
    /// [`EgmError::DatasetMissing`] (which explains how to fetch the data) when the file does
    /// not exist, and with [`EgmError::InvalidGridSize`] when its length is wrong.
    pub fn new(path: impl AsRef<Path>, model: EgmModel) -> Result<Self, EgmError> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                EgmError::DatasetMissing {
                    model,
                    path: path.to_path_buf(),
                }
            } else {
                EgmError::Io(err)
            }
        })?;
        let file_size = file.metadata()?.len();
        let (rows, cols_storage, lon_bins, format) = dimensions_from_file_size(model, file_size)?;
        let lat_step_deg = 180.0 / (rows as f64 - 1.0);
        let lon_step_deg = 360.0 / lon_bins as f64;

        Ok(Self {
            data_file: Some(Mutex::new(file)),
            geoid: Vec::new(),
            model,
            format,
            rows,
            cols_storage,
            lon_bins,
            lat_step_deg,
            lon_step_deg,
        })
    }

    /// Builds a fully in-memory geoid grid from raw dataset bytes (the same byte layout as the
    /// on-disk `.DAC`/`.dat` files). Useful for embedding the grid via `include_bytes!` so no
    /// runtime file path is required (e.g. wasm, or worker threads that cannot share a file
    /// handle). The returned grid holds no file handle and answers every query from memory.
    pub fn from_bytes(bytes: &[u8], model: EgmModel) -> Result<Self, EgmError> {
        let (rows, cols_storage, lon_bins, format) =
            dimensions_from_file_size(model, bytes.len() as u64)?;
        let lat_step_deg = 180.0 / (rows as f64 - 1.0);
        let lon_step_deg = 360.0 / lon_bins as f64;

        let geoid = read_grid_values(
            &mut io::Cursor::new(bytes),
            format,
            rows,
            cols_storage,
            lon_bins,
        )?;

        Ok(Self {
            data_file: None,
            geoid,
            model,
            format,
            rows,
            cols_storage,
            lon_bins,
            lat_step_deg,
            lon_step_deg,
        })
    }

    /// Opens the EGM96 15-arc-minute grid (`WW15MGH.DAC`) at `path`; shorthand for
    /// [`EgmGrid::new`] with [`EgmModel::Egm96`].
    pub fn egm96(path: impl AsRef<Path>) -> Result<Self, EgmError> {
        Self::new(path, EgmModel::Egm96)
    }

    /// Opens the EGM2008 2.5-arc-minute grid (`EGM2008_2_5.DAC`) at `path`; shorthand for
    /// [`EgmGrid::new`] with [`EgmModel::Egm2008`].
    pub fn egm2008(path: impl AsRef<Path>) -> Result<Self, EgmError> {
        Self::new(path, EgmModel::Egm2008)
    }

    /// The geoid model this grid implements — provenance for MSL values derived through it.
    pub fn model(&self) -> EgmModel {
        self.model
    }

    /// The grid's latitude spacing in degrees between adjacent rows (0.25° for EGM96,
    /// 2.5 arc-minutes for EGM2008).
    pub fn lat_step_deg(&self) -> f64 {
        self.lat_step_deg
    }

    /// The grid's longitude spacing in degrees between adjacent columns (0.25° for EGM96,
    /// 2.5 arc-minutes for EGM2008).
    pub fn lon_step_deg(&self) -> f64 {
        self.lon_step_deg
    }

    /// Reads the entire dataset into memory so subsequent queries never touch the filesystem
    /// (~8 MiB resident for EGM96, ~285 MiB for EGM2008, stored as `f64` per cell). A no-op
    /// when the grid is already in memory,
    /// e.g. after [`from_bytes`](Self::from_bytes) or a previous call.
    pub fn load_data(&mut self) -> Result<(), EgmError> {
        let Some(data_file) = self.data_file.as_ref() else {
            // No file handle means the grid was built fully in memory already.
            return Ok(());
        };
        let mut file = data_file.lock().unwrap_or_else(PoisonError::into_inner);
        file.seek(SeekFrom::Start(0))?;

        let geoid = read_grid_values(
            &mut *file,
            self.format,
            self.rows,
            self.cols_storage,
            self.lon_bins,
        )?;

        drop(file);
        self.geoid = geoid;
        Ok(())
    }

    /// Whether the full grid is resident in memory, either via [`load_data`](Self::load_data)
    /// or because the grid was built with [`from_bytes`](Self::from_bytes); when `false`,
    /// queries seek into the backing file on every lookup.
    pub fn is_loaded(&self) -> bool {
        !self.geoid.is_empty()
    }

    /// The geoid undulation N in meters stored at grid cell (`row`, `col`), where row 0 is
    /// 90° N (rows increase southward) and column 0 is 0° E (columns increase eastward). Reads
    /// from memory when loaded, otherwise seeks into the backing file; fails with
    /// [`EgmError::InvalidIndex`] when the cell is outside the grid.
    pub fn read_geoid_value(&self, row: usize, col: usize) -> Result<f64, EgmError> {
        if row >= self.rows || col >= self.lon_bins {
            return Err(EgmError::InvalidIndex {
                row,
                col,
                max_row: self.rows - 1,
                max_col: self.lon_bins - 1,
            });
        }

        let index = self.index(row, col);
        if !self.geoid.is_empty() {
            return Ok(self.geoid[index]);
        }

        let Some(data_file) = self.data_file.as_ref() else {
            return Err(EgmError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "geoid grid has no backing file and was not preloaded",
            )));
        };
        let mut file = data_file.lock().unwrap_or_else(PoisonError::into_inner);
        match self.format {
            GridFormat::Egm96I16Be => {
                file.seek(SeekFrom::Start((index * 2) as u64))?;
                let raw = file.read_i16::<BigEndian>()?;
                Ok(raw as f64 / 100.0)
            }
            GridFormat::Egm2008F32LeFortranSequential => {
                let record_size = (self.lon_bins * 4 + 8) as u64;
                let value_offset = row as u64 * record_size + 4 + col as u64 * 4;
                file.seek(SeekFrom::Start(value_offset))?;
                Ok(file.read_f32::<LittleEndian>()? as f64)
            }
        }
    }

    /// The (row, col) grid indices of the north-west corner of the grid cell containing
    /// (`lat`, `lon`) in degrees — the bracketing corner at or north/west of the point.
    /// Longitude is wrapped into [0, 360) first.
    pub fn lower_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        let lon = self.normalize_lon(lon)?;
        self.validate_lat(lat)?;
        let (row, col, _, _) = self.interpolation_cell(lat, lon);
        Ok((row, col))
    }

    /// The (row, col) grid indices of the south-east corner of the grid cell containing
    /// (`lat`, `lon`) in degrees — the bracketing corner diagonally opposite
    /// [`lower_indices`](Self::lower_indices). The column wraps across the antimeridian; the
    /// row is clamped at the south pole.
    pub fn upper_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        let (lower_row, lower_col) = self.lower_indices(lat, lon)?;
        let upper_row = (lower_row + 1).min(self.rows - 1);
        let upper_col = (lower_col + 1) % self.lon_bins;
        Ok((upper_row, upper_col))
    }

    /// The geoid undulation N in meters at (`lat`, `lon`) in degrees, using nearest-neighbor
    /// lookup (the value of the closest grid node, no interpolation). N relates the vertical
    /// datums as HAE = MSL + N, i.e. the height of the geoid above the WGS84 ellipsoid.
    pub fn offset(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        let lon = self.normalize_lon(lon)?;
        self.validate_lat(lat)?;

        let row = ((90.0 - lat) / self.lat_step_deg)
            .round()
            .clamp(0.0, (self.rows - 1) as f64) as usize;
        let col =
            ((lon / self.lon_step_deg).round() as i64).rem_euclid(self.lon_bins as i64) as usize;
        self.read_geoid_value(row, col)
    }

    /// The geoid undulation N in meters at (`lat`, `lon`) in degrees, bilinearly interpolated
    /// from the four grid nodes bracketing the point (HAE = MSL + N). Smoother than
    /// [`offset`](Self::offset); the usual accuracy/cost trade-off for geoid lookups.
    pub fn offset_bilinear(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        let lon = self.normalize_lon(lon)?;
        self.validate_lat(lat)?;

        let (lower_row, lower_col, tx, ty) = self.interpolation_cell(lat, lon);
        let upper_row = (lower_row + 1).min(self.rows - 1);
        let upper_col = (lower_col + 1) % self.lon_bins;

        let nw = self.read_geoid_value(lower_row, lower_col)?;
        let ne = self.read_geoid_value(lower_row, upper_col)?;
        let sw = self.read_geoid_value(upper_row, lower_col)?;
        let se = self.read_geoid_value(upper_row, upper_col)?;

        let north = nw * (1.0 - tx) + ne * tx;
        let south = sw * (1.0 - tx) + se * tx;
        Ok(north * (1.0 - ty) + south * ty)
    }

    /// The geoid undulation N in meters at (`lat`, `lon`) in degrees, bicubically interpolated
    /// from the surrounding 4x4 block of grid nodes (HAE = MSL + N). Smoothest of the three
    /// variants at 16 grid reads per query; columns wrap across the antimeridian and rows are
    /// clamped at the poles.
    pub fn offset_bicubic(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        let lon = self.normalize_lon(lon)?;
        self.validate_lat(lat)?;

        let (lower_row, lower_col, tx, ty) = self.interpolation_cell(lat, lon);
        let row_indices = [
            lower_row.saturating_sub(1),
            lower_row,
            (lower_row + 1).min(self.rows - 1),
            (lower_row + 2).min(self.rows - 1),
        ];
        let col_indices = [
            (lower_col + self.lon_bins - 1) % self.lon_bins,
            lower_col,
            (lower_col + 1) % self.lon_bins,
            (lower_col + 2) % self.lon_bins,
        ];

        let mut grid = [[0.0_f64; 4]; 4];
        for (r, row_idx) in row_indices.iter().enumerate() {
            for (c, col_idx) in col_indices.iter().enumerate() {
                grid[r][c] = self.read_geoid_value(*row_idx, *col_idx)?;
            }
        }

        Ok(bicubic_unit(tx, ty, grid))
    }

    fn interpolation_cell(&self, lat: f64, lon: f64) -> (usize, usize, f64, f64) {
        let lower_row = ((90.0 - lat) / self.lat_step_deg)
            .floor()
            .clamp(0.0, (self.rows - 2) as f64) as usize;
        let lower_col =
            ((lon / self.lon_step_deg).floor() as i64).rem_euclid(self.lon_bins as i64) as usize;

        let north_lat = 90.0 - lower_row as f64 * self.lat_step_deg;
        let west_lon = lower_col as f64 * self.lon_step_deg;

        let tx = ((lon - west_lon) / self.lon_step_deg).clamp(0.0, 1.0);
        let ty = ((north_lat - lat) / self.lat_step_deg).clamp(0.0, 1.0);
        (lower_row, lower_col, tx, ty)
    }

    fn validate_lat(&self, lat: f64) -> Result<(), EgmError> {
        if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
            return Err(EgmError::InvalidLatitude(lat));
        }
        Ok(())
    }

    fn normalize_lon(&self, lon: f64) -> Result<f64, EgmError> {
        if !lon.is_finite() {
            return Err(EgmError::InvalidLongitude(lon));
        }
        let mut normalized = lon % 360.0;
        if normalized < 0.0 {
            normalized += 360.0;
        }
        if normalized >= 360.0 {
            normalized = 0.0;
        }
        Ok(normalized)
    }

    fn index(&self, row: usize, col: usize) -> usize {
        row * self.cols_storage + col
    }
}

/// Reads a full geoid grid from any reader, shared by the file-backed [`EgmGrid::load_data`]
/// and the in-memory [`EgmGrid::from_bytes`]. Values are stored row-major with `cols_storage`
/// stride (equal to `lon_bins` for the supported datasets).
fn read_grid_values<R: Read>(
    reader: &mut R,
    format: GridFormat,
    rows: usize,
    cols_storage: usize,
    lon_bins: usize,
) -> Result<Vec<f64>, EgmError> {
    let mut geoid = vec![0.0_f64; rows * lon_bins];
    let index = |row: usize, col: usize| row * cols_storage + col;

    match format {
        GridFormat::Egm96I16Be => {
            for row in 0..rows {
                for col in 0..lon_bins {
                    let raw = reader.read_i16::<BigEndian>()?;
                    geoid[index(row, col)] = raw as f64 / 100.0;
                }
            }
        }
        GridFormat::Egm2008F32LeFortranSequential => {
            let expected_record_bytes = (lon_bins * 4) as u32;
            for row in 0..rows {
                let leading_record_bytes = reader.read_u32::<LittleEndian>()?;
                if leading_record_bytes != expected_record_bytes {
                    return Err(EgmError::InvalidGridSize {
                        model: EgmModel::Egm2008,
                        expected_bytes: rows * (lon_bins * 4 + 8),
                        actual_bytes: rows as u64 * leading_record_bytes as u64,
                    });
                }

                for col in 0..lon_bins {
                    geoid[index(row, col)] = reader.read_f32::<LittleEndian>()? as f64;
                }

                let trailing_record_bytes = reader.read_u32::<LittleEndian>()?;
                if trailing_record_bytes != expected_record_bytes {
                    return Err(EgmError::InvalidGridSize {
                        model: EgmModel::Egm2008,
                        expected_bytes: rows * (lon_bins * 4 + 8),
                        actual_bytes: rows as u64 * trailing_record_bytes as u64,
                    });
                }
            }
        }
    }

    Ok(geoid)
}

fn dimensions_from_file_size(
    model: EgmModel,
    file_size_bytes: u64,
) -> Result<(usize, usize, usize, GridFormat), EgmError> {
    match model {
        EgmModel::Egm96 => {
            let expected = EGM96_ROWS * EGM96_COLS * 2;
            if file_size_bytes as usize != expected {
                return Err(EgmError::InvalidGridSize {
                    model,
                    expected_bytes: expected,
                    actual_bytes: file_size_bytes,
                });
            }
            Ok((EGM96_ROWS, EGM96_COLS, EGM96_COLS, GridFormat::Egm96I16Be))
        }
        EgmModel::Egm2008 => {
            if file_size_bytes as usize != EGM2008_ROWS * (EGM2008_COLS * 4 + 8) {
                return Err(EgmError::InvalidGridSize {
                    model,
                    expected_bytes: EGM2008_ROWS * (EGM2008_COLS * 4 + 8),
                    actual_bytes: file_size_bytes,
                });
            }
            Ok((
                EGM2008_ROWS,
                EGM2008_COLS,
                EGM2008_COLS,
                GridFormat::Egm2008F32LeFortranSequential,
            ))
        }
    }
}

/// The EGM96 15-arc-minute geoid — NGA's `WW15MGH.DAC` grid of big-endian `i16` undulations in
/// centimeters. Small enough (~2 MiB) to ship inside the binary via the `embedded-egm96`
/// feature and [`EGM96::embedded`]; for higher resolution use [`EGM2008`].
#[derive(Debug)]
pub struct EGM96 {
    inner: EgmGrid,
}

impl EGM96 {
    /// Opens the EGM96 grid file (`WW15MGH.DAC`) at `path`, validating its size but deferring
    /// all value reads to query time; call [`load_data`](Self::load_data) to bring the grid
    /// fully into memory. A missing file fails with [`EgmError::DatasetMissing`], whose message
    /// includes the download command.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, EgmError> {
        Ok(Self {
            inner: EgmGrid::egm96(path)?,
        })
    }

    /// Builds a fully in-memory EGM96 geoid from raw `WW15MGH.DAC` bytes (e.g. embedded via
    /// `include_bytes!`), requiring no runtime file path.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EgmError> {
        Ok(Self {
            inner: EgmGrid::from_bytes(bytes, EgmModel::Egm96)?,
        })
    }

    /// The geoid model this grid implements — provenance for MSL values derived through it
    /// (see `crate::altitude::VerticalFrame::Msl` on why that matters at centimeter accuracy).
    pub fn model(&self) -> EgmModel {
        self.inner.model()
    }

    /// Builds the EGM96 geoid from the grid embedded in the binary at compile time — no runtime
    /// data path or download step. Adds ~2 MiB to the binary.
    ///
    /// The embedded grid is the crate's own `data/WW15MGH.DAC` (see the repository `NOTICE` for
    /// provenance: NGA/NASA EGM96, a U.S. Government work).
    #[cfg(feature = "embedded-egm96")]
    pub fn embedded() -> Result<Self, EgmError> {
        Self::from_bytes(include_bytes!("../data/WW15MGH.DAC"))
    }

    /// Reads the entire grid into memory (~8 MiB resident) so subsequent queries never touch
    /// the filesystem; a no-op when the grid is already in memory.
    pub fn load_data(&mut self) -> Result<(), EgmError> {
        self.inner.load_data()
    }

    /// The EGM96 geoid undulation N in meters at (`lat`, `lon`) in degrees, using
    /// nearest-neighbor lookup. N relates the vertical datums as HAE = MSL + N.
    pub fn offset(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset(lat, lon)
    }

    /// The EGM96 geoid undulation N in meters at (`lat`, `lon`) in degrees, bilinearly
    /// interpolated from the four bracketing grid nodes (HAE = MSL + N).
    pub fn offset_bilinear(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset_bilinear(lat, lon)
    }

    /// The EGM96 geoid undulation N in meters at (`lat`, `lon`) in degrees, bicubically
    /// interpolated from the surrounding 4x4 block of grid nodes (HAE = MSL + N).
    pub fn offset_bicubic(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset_bicubic(lat, lon)
    }

    /// The geoid undulation N in meters stored at grid cell (`row`, `col`), where row 0 is
    /// 90° N and column 0 is 0° E; see [`EgmGrid::read_geoid_value`].
    pub fn read_geoid_value(&self, row: usize, col: usize) -> Result<f64, EgmError> {
        self.inner.read_geoid_value(row, col)
    }

    /// The (row, col) indices of the north-west corner of the grid cell containing
    /// (`lat`, `lon`) in degrees; see [`EgmGrid::lower_indices`].
    pub fn lower_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        self.inner.lower_indices(lat, lon)
    }

    /// The (row, col) indices of the south-east corner of the grid cell containing
    /// (`lat`, `lon`) in degrees; see [`EgmGrid::upper_indices`].
    pub fn upper_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        self.inner.upper_indices(lat, lon)
    }
}

/// The EGM2008 2.5-arc-minute geoid. Higher resolution than [`EGM96`], but its official NGA grid
/// is ~142 MiB on disk (~285 MiB resident once loaded), so it cannot be embedded in the crate the
/// way `embedded-egm96` embeds EGM96. Fetch it with one command (also printed by the error when
/// the file is missing):
///
/// ```sh
/// curl -fsSL https://raw.githubusercontent.com/alpacasweater/small_world/main/scripts/download_geoid_data.sh | bash -s -- --model egm2008
/// ```
///
/// or in a checkout: `./scripts/download_geoid_data.sh --model egm2008`.
#[derive(Debug)]
pub struct EGM2008 {
    inner: EgmGrid,
}

impl EGM2008 {
    /// Opens the EGM2008 grid file (`EGM2008_2_5.DAC`) at `path`, validating its size but
    /// deferring all value reads to query time; call [`load_data`](Self::load_data) to bring
    /// the grid fully into memory. A missing file fails with [`EgmError::DatasetMissing`],
    /// whose message includes the download command.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, EgmError> {
        Ok(Self {
            inner: EgmGrid::egm2008(path)?,
        })
    }

    /// The geoid model this grid implements — provenance for MSL values derived through it.
    pub fn model(&self) -> EgmModel {
        self.inner.model()
    }

    /// Builds a fully in-memory EGM2008 geoid from raw grid bytes (the same byte layout as the
    /// official `EGM2008_2_5.DAC` file: little-endian f32 rows with Fortran record markers).
    /// Useful when the grid is delivered by something other than the filesystem — an application
    /// cache, an object store, a memory-mapped region.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EgmError> {
        Ok(Self {
            inner: EgmGrid::from_bytes(bytes, EgmModel::Egm2008)?,
        })
    }

    /// Reads the entire grid into memory (~285 MiB resident) so subsequent queries never touch
    /// the filesystem; a no-op when the grid is already in memory.
    pub fn load_data(&mut self) -> Result<(), EgmError> {
        self.inner.load_data()
    }

    /// The EGM2008 geoid undulation N in meters at (`lat`, `lon`) in degrees, using
    /// nearest-neighbor lookup. N relates the vertical datums as HAE = MSL + N.
    pub fn offset(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset(lat, lon)
    }

    /// The EGM2008 geoid undulation N in meters at (`lat`, `lon`) in degrees, bilinearly
    /// interpolated from the four bracketing grid nodes (HAE = MSL + N).
    pub fn offset_bilinear(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset_bilinear(lat, lon)
    }

    /// The EGM2008 geoid undulation N in meters at (`lat`, `lon`) in degrees, bicubically
    /// interpolated from the surrounding 4x4 block of grid nodes (HAE = MSL + N).
    pub fn offset_bicubic(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset_bicubic(lat, lon)
    }

    /// The geoid undulation N in meters stored at grid cell (`row`, `col`), where row 0 is
    /// 90° N and column 0 is 0° E; see [`EgmGrid::read_geoid_value`].
    pub fn read_geoid_value(&self, row: usize, col: usize) -> Result<f64, EgmError> {
        self.inner.read_geoid_value(row, col)
    }

    /// The (row, col) indices of the north-west corner of the grid cell containing
    /// (`lat`, `lon`) in degrees; see [`EgmGrid::lower_indices`].
    pub fn lower_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        self.inner.lower_indices(lat, lon)
    }

    /// The (row, col) indices of the south-east corner of the grid cell containing
    /// (`lat`, `lon`) in degrees; see [`EgmGrid::upper_indices`].
    pub fn upper_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        self.inner.upper_indices(lat, lon)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        dimensions_from_file_size, EgmModel, GridFormat, EGM2008_COLS, EGM2008_ROWS, EGM96_COLS,
        EGM96_ROWS,
    };

    #[test]
    fn egm96_dimensions_match() {
        let size = (EGM96_ROWS * EGM96_COLS * 2) as u64;
        let dims = dimensions_from_file_size(EgmModel::Egm96, size).unwrap();
        assert_eq!(
            dims,
            (EGM96_ROWS, EGM96_COLS, EGM96_COLS, GridFormat::Egm96I16Be)
        );
    }

    #[test]
    fn egm96_dimensions_reject_invalid_size() {
        let result = dimensions_from_file_size(EgmModel::Egm96, 10);
        assert!(result.is_err());
    }

    #[test]
    fn egm2008_dimensions_accept_8640_columns() {
        let size = (EGM2008_ROWS * (EGM2008_COLS * 4 + 8)) as u64;
        let dims = dimensions_from_file_size(EgmModel::Egm2008, size).unwrap();
        assert_eq!(
            dims,
            (
                EGM2008_ROWS,
                EGM2008_COLS,
                EGM2008_COLS,
                GridFormat::Egm2008F32LeFortranSequential
            )
        );
    }

    #[test]
    fn egm2008_dimensions_reject_invalid_size() {
        let result = dimensions_from_file_size(EgmModel::Egm2008, 1234);
        assert!(result.is_err());
    }

    #[test]
    fn from_bytes_matches_file_backed_egm96() {
        use super::EGM96;
        use std::path::Path;

        let path = Path::new("data/WW15MGH.DAC");
        let raw = match std::fs::read(path) {
            Ok(raw) => raw,
            // Skip when the dataset is not vendored in this checkout.
            Err(_) => return,
        };

        let from_bytes = EGM96::from_bytes(&raw).expect("in-memory EGM96 should parse");
        let from_file = EGM96::new(path).expect("file-backed EGM96 should open");

        for (lat, lon) in [
            (0.0, 0.0),
            (27.9881, 86.925),
            (4.75, 78.75),
            (-45.0, -170.0),
        ] {
            let memory = from_bytes.offset_bilinear(lat, lon).unwrap();
            let file = from_file.offset_bilinear(lat, lon).unwrap();
            assert!(
                (memory - file).abs() < 1e-9,
                "in-memory vs file geoid mismatch at ({lat}, {lon}): {memory} vs {file}"
            );
        }
    }

    #[test]
    fn geoid_types_are_send_and_sync() {
        // Locked in at compile time: geoids are shared across threads in exactly the systems
        // this crate targets (robotics processes, ECS resources). The file-backed mode uses a
        // Mutex internally, so this holds for every construction path, not just from_bytes.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<super::EGM96>();
        assert_send_sync::<super::EGM2008>();
        assert_send_sync::<super::EgmGrid>();
    }

    #[test]
    fn missing_dataset_error_explains_its_own_fix() {
        use super::{EgmError, EGM2008, EGM96};
        use std::path::Path;

        // The one part of "trivial for new users" a doc can't provide: the failure itself must
        // carry the fix. Both models, both hints.
        let err = EGM2008::new(Path::new("data/definitely_not_here.DAC")).unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(err, EgmError::DatasetMissing { .. }),
            "got {err:?}"
        );
        assert!(
            msg.contains("download_geoid_data.sh"),
            "no fetch command: {msg}"
        );
        assert!(
            msg.contains("--model egm2008"),
            "wrong model in command: {msg}"
        );
        assert!(
            msg.contains("definitely_not_here.DAC"),
            "path missing: {msg}"
        );

        let msg96 = EGM96::new(Path::new("data/also_not_here.DAC"))
            .unwrap_err()
            .to_string();
        assert!(msg96.contains("--model egm96"), "wrong model: {msg96}");
        assert!(
            msg96.contains("embedded-egm96"),
            "EGM96 must advertise the zero-download alternative: {msg96}"
        );

        // Only absence maps to the guidance error; other I/O failures must stay I/O errors.
        // (A directory in place of the file opens fine on Unix but fails at metadata/read; the
        // NotFound mapping alone is what we assert here.)
    }

    /// Full-size synthetic EGM2008 grid: exercises the real 2.5-minute parse path (record
    /// markers, layout, value decoding) without the 142 MiB NGA dataset, which cannot be
    /// committed and is not present on CI. Values are chosen to be exact in f32.
    #[test]
    fn egm2008_from_bytes_parses_a_synthetic_grid_and_rejects_corruption() {
        use super::{EGM2008, EGM2008_COLS, EGM2008_ROWS};

        let value = |row: usize, col: usize| ((row % 200) as f32) - ((col % 100) as f32) * 0.5;

        let record = (EGM2008_COLS * 4) as u32;
        let mut bytes = Vec::with_capacity(EGM2008_ROWS * (EGM2008_COLS * 4 + 8));
        for row in 0..EGM2008_ROWS {
            bytes.extend_from_slice(&record.to_le_bytes());
            for col in 0..EGM2008_COLS {
                bytes.extend_from_slice(&value(row, col).to_le_bytes());
            }
            bytes.extend_from_slice(&record.to_le_bytes());
        }

        let grid = EGM2008::from_bytes(&bytes).expect("synthetic grid should parse");
        for (row, col) in [
            (0, 0),
            (0, EGM2008_COLS - 1),
            (EGM2008_ROWS - 1, 0),
            (EGM2008_ROWS - 1, EGM2008_COLS - 1),
            (EGM2008_ROWS / 2, EGM2008_COLS / 2),
        ] {
            let got = grid.read_geoid_value(row, col).unwrap();
            let expected = value(row, col) as f64;
            assert!(
                (got - expected).abs() < 1e-12,
                "({row}, {col}): got {got}, expected {expected}"
            );
        }

        // A corrupted Fortran record marker must be rejected, not silently misread as data.
        bytes[0] ^= 0xFF;
        assert!(
            EGM2008::from_bytes(&bytes).is_err(),
            "corrupt record marker must fail"
        );
    }

    /// The embedded grid must reproduce the published NGA EGM96 reference undulations. Anchors
    /// (sub-metre agreement expected with bilinear interpolation):
    /// (0°N, 0°E) = +17.16 m; Everest (27.9881°N, 86.925°E) = −28.74 m.
    #[cfg(feature = "embedded-egm96")]
    #[test]
    fn embedded_grid_matches_reference_undulations() {
        use super::EGM96;

        let geoid = EGM96::embedded().expect("embedded EGM96 should parse");
        for (lat, lon, expected_m) in [(0.0, 0.0, 17.16), (27.9881, 86.925, -28.74)] {
            let n = geoid.offset_bilinear(lat, lon).unwrap();
            assert!(
                (n - expected_m).abs() < 1.0,
                "undulation at ({lat}, {lon}): got {n}, reference {expected_m}"
            );
        }
    }
}
