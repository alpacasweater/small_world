use std::cell::RefCell;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use std::path::Path;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EgmModel {
    Egm96,
    Egm2008,
}

impl EgmModel {
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

    pub fn canonical_filename(self) -> &'static str {
        match self {
            EgmModel::Egm96 => "WW15MGH.DAC",
            EgmModel::Egm2008 => "EGM2008_2_5.DAC",
        }
    }
}

#[derive(Debug)]
pub enum EgmError {
    Io(io::Error),
    InvalidLatitude(f64),
    InvalidLongitude(f64),
    InvalidIndex {
        row: usize,
        col: usize,
        max_row: usize,
        max_col: usize,
    },
    InvalidGridSize {
        model: EgmModel,
        expected_bytes: usize,
        actual_bytes: u64,
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
        }
    }
}

impl From<io::Error> for EgmError {
    fn from(value: io::Error) -> Self {
        EgmError::Io(value)
    }
}

#[derive(Debug)]
pub struct EgmGrid {
    data_file: RefCell<File>,
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
    pub fn new(path: &Path, model: EgmModel) -> Result<Self, EgmError> {
        let file = File::open(path)?;
        let file_size = file.metadata()?.len();
        let (rows, cols_storage, lon_bins, format) = dimensions_from_file_size(model, file_size)?;
        let lat_step_deg = 180.0 / (rows as f64 - 1.0);
        let lon_step_deg = 360.0 / lon_bins as f64;

        Ok(Self {
            data_file: RefCell::new(file),
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

    pub fn egm96(path: &Path) -> Result<Self, EgmError> {
        Self::new(path, EgmModel::Egm96)
    }

    pub fn egm2008(path: &Path) -> Result<Self, EgmError> {
        Self::new(path, EgmModel::Egm2008)
    }

    pub fn model(&self) -> EgmModel {
        self.model
    }

    pub fn lat_step_deg(&self) -> f64 {
        self.lat_step_deg
    }

    pub fn lon_step_deg(&self) -> f64 {
        self.lon_step_deg
    }

    pub fn load_data(&mut self) -> Result<(), EgmError> {
        let mut file = self.data_file.borrow_mut();
        file.seek(SeekFrom::Start(0))?;

        let mut geoid = vec![0.0_f64; self.rows * self.lon_bins];
        match self.format {
            GridFormat::Egm96I16Be => {
                for row in 0..self.rows {
                    for col in 0..self.lon_bins {
                        let raw = file.read_i16::<BigEndian>()?;
                        geoid[self.index(row, col)] = raw as f64 / 100.0;
                    }
                }
            }
            GridFormat::Egm2008F32LeFortranSequential => {
                let expected_record_bytes = (self.lon_bins * 4) as u32;
                for row in 0..self.rows {
                    let leading_record_bytes = file.read_u32::<LittleEndian>()?;
                    if leading_record_bytes != expected_record_bytes {
                        return Err(EgmError::InvalidGridSize {
                            model: self.model,
                            expected_bytes: self.rows * (self.lon_bins * 4 + 8),
                            actual_bytes: self.rows as u64 * leading_record_bytes as u64,
                        });
                    }

                    for col in 0..self.lon_bins {
                        geoid[self.index(row, col)] = file.read_f32::<LittleEndian>()? as f64;
                    }

                    let trailing_record_bytes = file.read_u32::<LittleEndian>()?;
                    if trailing_record_bytes != expected_record_bytes {
                        return Err(EgmError::InvalidGridSize {
                            model: self.model,
                            expected_bytes: self.rows * (self.lon_bins * 4 + 8),
                            actual_bytes: self.rows as u64 * trailing_record_bytes as u64,
                        });
                    }
                }
            }
        }

        self.geoid = geoid;
        Ok(())
    }

    pub fn is_loaded(&self) -> bool {
        !self.geoid.is_empty()
    }

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

        let mut file = self.data_file.borrow_mut();
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

    pub fn lower_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        let lon = self.normalize_lon(lon)?;
        self.validate_lat(lat)?;
        let (row, col, _, _) = self.interpolation_cell(lat, lon);
        Ok((row, col))
    }

    pub fn upper_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        let (lower_row, lower_col) = self.lower_indices(lat, lon)?;
        let upper_row = (lower_row + 1).min(self.rows - 1);
        let upper_col = (lower_col + 1) % self.lon_bins;
        Ok((upper_row, upper_col))
    }

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

pub struct EGM96 {
    inner: EgmGrid,
}

impl EGM96 {
    pub fn new(path: &Path) -> Result<Self, EgmError> {
        Ok(Self {
            inner: EgmGrid::egm96(path)?,
        })
    }

    pub fn load_data(&mut self) -> Result<(), EgmError> {
        self.inner.load_data()
    }

    pub fn offset(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset(lat, lon)
    }

    pub fn offset_bilinear(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset_bilinear(lat, lon)
    }

    pub fn offset_bicubic(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset_bicubic(lat, lon)
    }

    pub fn read_geoid_value(&self, row: usize, col: usize) -> Result<f64, EgmError> {
        self.inner.read_geoid_value(row, col)
    }

    pub fn lower_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        self.inner.lower_indices(lat, lon)
    }

    pub fn upper_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        self.inner.upper_indices(lat, lon)
    }
}

pub struct EGM2008 {
    inner: EgmGrid,
}

impl EGM2008 {
    pub fn new(path: &Path) -> Result<Self, EgmError> {
        Ok(Self {
            inner: EgmGrid::egm2008(path)?,
        })
    }

    pub fn load_data(&mut self) -> Result<(), EgmError> {
        self.inner.load_data()
    }

    pub fn offset(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset(lat, lon)
    }

    pub fn offset_bilinear(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset_bilinear(lat, lon)
    }

    pub fn offset_bicubic(&self, lat: f64, lon: f64) -> Result<f64, EgmError> {
        self.inner.offset_bicubic(lat, lon)
    }

    pub fn read_geoid_value(&self, row: usize, col: usize) -> Result<f64, EgmError> {
        self.inner.read_geoid_value(row, col)
    }

    pub fn lower_indices(&self, lat: f64, lon: f64) -> Result<(usize, usize), EgmError> {
        self.inner.lower_indices(lat, lon)
    }

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
}
