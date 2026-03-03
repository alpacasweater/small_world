use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::height::Interpolation;
use crate::interpolate::bicubic_unit;

const VOID_SAMPLE: i16 = -32768;
const QUERY_EDGE_EPSILON: f64 = 1e-12;
const DEFAULT_MAX_CACHED_TILES: usize = 64;

#[derive(Debug)]
pub enum TerrainError {
    Io(io::Error),
    InvalidLatitude(f64),
    InvalidLongitude(f64),
    TileNotFound {
        path: PathBuf,
    },
    InvalidTileSize {
        path: PathBuf,
        bytes: usize,
    },
    VoidSample {
        tile_lat_floor_deg: i32,
        tile_lon_floor_deg: i32,
        row: usize,
        col: usize,
    },
    VoidSampleNoNeighbor {
        tile_lat_floor_deg: i32,
        tile_lon_floor_deg: i32,
        row: usize,
        col: usize,
        max_radius_cells: usize,
    },
    CacheLockPoisoned,
}

impl Display for TerrainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TerrainError::Io(err) => write!(f, "I/O error: {err}"),
            TerrainError::InvalidLatitude(lat) => {
                write!(f, "latitude must be finite and within [-90, 90], got {lat}")
            }
            TerrainError::InvalidLongitude(lon) => {
                write!(f, "longitude must be finite, got {lon}")
            }
            TerrainError::TileNotFound { path } => {
                write!(f, "terrain tile not found: {}", path.display())
            }
            TerrainError::InvalidTileSize { path, bytes } => {
                write!(
                    f,
                    "invalid HGT tile size for {}: {} bytes (expected 2*n*n bytes)",
                    path.display(),
                    bytes
                )
            }
            TerrainError::VoidSample {
                tile_lat_floor_deg,
                tile_lon_floor_deg,
                row,
                col,
            } => {
                write!(
                    f,
                    "void terrain sample at tile ({tile_lat_floor_deg}, {tile_lon_floor_deg}), row={row}, col={col}"
                )
            }
            TerrainError::VoidSampleNoNeighbor {
                tile_lat_floor_deg,
                tile_lon_floor_deg,
                row,
                col,
                max_radius_cells,
            } => {
                write!(
                    f,
                    "void terrain sample at tile ({tile_lat_floor_deg}, {tile_lon_floor_deg}), row={row}, col={col} with no valid neighbor within radius {max_radius_cells}"
                )
            }
            TerrainError::CacheLockPoisoned => {
                write!(f, "terrain cache lock poisoned")
            }
        }
    }
}

impl Error for TerrainError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TerrainError::Io(err) => Some(err),
            TerrainError::InvalidLatitude(_) => None,
            TerrainError::InvalidLongitude(_) => None,
            TerrainError::TileNotFound { .. } => None,
            TerrainError::InvalidTileSize { .. } => None,
            TerrainError::VoidSample { .. } => None,
            TerrainError::VoidSampleNoNeighbor { .. } => None,
            TerrainError::CacheLockPoisoned => None,
        }
    }
}

impl From<io::Error> for TerrainError {
    fn from(value: io::Error) -> Self {
        TerrainError::Io(value)
    }
}

/// Strategy for handling void (`-32768`) DEM samples.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoidPolicy {
    /// Return an error on void samples.
    Error,
    /// Treat void samples as zero meters MSL.
    Zero,
    /// Search for the nearest valid sample in the same tile up to a radius.
    NearestValid { max_radius_cells: usize },
}

impl Default for VoidPolicy {
    fn default() -> Self {
        Self::Error
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TileKey {
    lat_floor_deg: i32,
    lon_floor_deg: i32,
}

impl TileKey {
    fn filename(self) -> String {
        let lat_prefix = if self.lat_floor_deg >= 0 { 'N' } else { 'S' };
        let lon_prefix = if self.lon_floor_deg >= 0 { 'E' } else { 'W' };
        format!(
            "{lat_prefix}{:02}{lon_prefix}{:03}.hgt",
            self.lat_floor_deg.abs(),
            self.lon_floor_deg.abs()
        )
    }
}

#[derive(Debug)]
struct HgtTile {
    key: TileKey,
    side: usize,
    samples: Vec<i16>,
}

impl HgtTile {
    fn from_file(path: &Path, key: TileKey) -> Result<Self, TerrainError> {
        let bytes = fs::read(path)?;
        if bytes.len() % 2 != 0 {
            return Err(TerrainError::InvalidTileSize {
                path: path.to_path_buf(),
                bytes: bytes.len(),
            });
        }

        let sample_count = bytes.len() / 2;
        let side = (sample_count as f64).sqrt() as usize;
        if side < 2 || side * side != sample_count {
            return Err(TerrainError::InvalidTileSize {
                path: path.to_path_buf(),
                bytes: bytes.len(),
            });
        }

        let mut samples = Vec::with_capacity(sample_count);
        for chunk in bytes.chunks_exact(2) {
            samples.push(i16::from_be_bytes([chunk[0], chunk[1]]));
        }

        Ok(Self { key, side, samples })
    }

    fn sample_raw(&self, row: usize, col: usize) -> i16 {
        self.samples[row * self.side + col]
    }
}

#[derive(Debug)]
struct CacheState {
    tiles: HashMap<TileKey, Arc<HgtTile>>,
    lru: VecDeque<TileKey>,
    max_tiles: usize,
}

impl CacheState {
    fn new(max_tiles: usize) -> Self {
        Self {
            tiles: HashMap::new(),
            lru: VecDeque::new(),
            max_tiles: max_tiles.max(1),
        }
    }

    fn get(&mut self, key: TileKey) -> Option<Arc<HgtTile>> {
        let tile = self.tiles.get(&key).cloned();
        if tile.is_some() {
            self.touch(key);
        }
        tile
    }

    fn insert(&mut self, key: TileKey, tile: Arc<HgtTile>) {
        if let std::collections::hash_map::Entry::Occupied(mut entry) = self.tiles.entry(key) {
            entry.insert(tile);
            self.touch(key);
            return;
        }

        while self.tiles.len() >= self.max_tiles {
            if let Some(evict_key) = self.lru.pop_back() {
                self.tiles.remove(&evict_key);
            } else {
                break;
            }
        }
        self.tiles.insert(key, tile);
        self.touch(key);
    }

    fn clear(&mut self) {
        self.tiles.clear();
        self.lru.clear();
    }

    fn len(&self) -> usize {
        self.tiles.len()
    }

    fn capacity(&self) -> usize {
        self.max_tiles
    }

    fn set_capacity(&mut self, max_tiles: usize) {
        self.max_tiles = max_tiles.max(1);
        while self.tiles.len() > self.max_tiles {
            if let Some(evict_key) = self.lru.pop_back() {
                self.tiles.remove(&evict_key);
            } else {
                break;
            }
        }
    }

    fn touch(&mut self, key: TileKey) {
        if let Some(index) = self.lru.iter().position(|existing| *existing == key) {
            self.lru.remove(index);
        }
        self.lru.push_front(key);
    }
}

#[derive(Clone, Copy, Debug)]
struct InterpolationCell {
    base_key: TileKey,
    base_side: usize,
    row0: isize,
    col0: isize,
    tx: f64,
    ty: f64,
}

/// Terrain dataset reader for SRTM `.hgt` tiles.
///
/// Returned elevations are orthometric terrain heights in meters above mean sea level (MSL).
#[derive(Debug)]
pub struct SrtmDataset {
    root: PathBuf,
    cache: Mutex<CacheState>,
    loaded_tile_count: AtomicUsize,
    void_policy: VoidPolicy,
}

impl SrtmDataset {
    /// Creates a terrain dataset rooted at a directory of `.hgt` tiles.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            cache: Mutex::new(CacheState::new(DEFAULT_MAX_CACHED_TILES)),
            loaded_tile_count: AtomicUsize::new(0),
            void_policy: VoidPolicy::Error,
        }
    }

    /// Returns a dataset with a configured void handling strategy.
    pub fn with_void_policy(mut self, policy: VoidPolicy) -> Self {
        self.void_policy = policy;
        self
    }

    /// Returns a dataset with bounded tile cache size.
    pub fn with_max_cached_tiles(self, max_tiles: usize) -> Self {
        self.set_max_cached_tiles(max_tiles);
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    pub fn cached_tile_count(&self) -> usize {
        self.cache.lock().map(|cache| cache.len()).unwrap_or(0)
    }

    pub fn loaded_tile_count(&self) -> usize {
        self.loaded_tile_count.load(Ordering::Relaxed)
    }

    pub fn cache_capacity(&self) -> usize {
        self.cache.lock().map(|cache| cache.capacity()).unwrap_or(0)
    }

    pub fn set_max_cached_tiles(&self, max_tiles: usize) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.set_capacity(max_tiles);
        }
    }

    pub fn void_policy(&self) -> VoidPolicy {
        self.void_policy
    }

    /// Terrain elevation in MSL meters using nearest-neighbor interpolation.
    pub fn elevation_msl(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, TerrainError> {
        self.query(lat_deg, lon_deg, Interpolation::Nearest)
    }

    /// Terrain elevation in MSL meters using bilinear interpolation.
    pub fn elevation_msl_bilinear(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, TerrainError> {
        self.query(lat_deg, lon_deg, Interpolation::Bilinear)
    }

    /// Terrain elevation in MSL meters using bicubic interpolation.
    pub fn elevation_msl_bicubic(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, TerrainError> {
        self.query(lat_deg, lon_deg, Interpolation::Bicubic)
    }

    /// Terrain elevation in MSL meters with caller-selected interpolation mode.
    pub fn elevation_msl_with_interpolation(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        interpolation: Interpolation,
    ) -> Result<f64, TerrainError> {
        self.query(lat_deg, lon_deg, interpolation)
    }

    fn query(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        interpolation: Interpolation,
    ) -> Result<f64, TerrainError> {
        match interpolation {
            Interpolation::Nearest => self.nearest_elevation(lat_deg, lon_deg),
            Interpolation::Bilinear => self.bilinear_elevation(lat_deg, lon_deg),
            Interpolation::Bicubic => self.bicubic_elevation(lat_deg, lon_deg),
        }
    }

    fn nearest_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, TerrainError> {
        let (lat_deg, lon_deg) = normalize_query(lat_deg, lon_deg)?;
        let key = tile_key(lat_deg, lon_deg);
        let tile = self.get_or_load_tile(key)?;
        let (x, y) = tile_coordinates(lat_deg, lon_deg, key, tile.side);
        let row = y.round().clamp(0.0, (tile.side - 1) as f64) as usize;
        let col = x.round().clamp(0.0, (tile.side - 1) as f64) as usize;
        self.resolve_sample_value(&tile, row, col)
    }

    fn bilinear_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, TerrainError> {
        let cell = self.interpolation_cell(lat_deg, lon_deg)?;

        let nw = self.sample_by_offset(cell.base_key, cell.base_side, cell.row0, cell.col0)?;
        let north = if cell.tx == 0.0 {
            nw
        } else {
            let ne =
                self.sample_by_offset(cell.base_key, cell.base_side, cell.row0, cell.col0 + 1)?;
            nw * (1.0 - cell.tx) + ne * cell.tx
        };
        if cell.ty == 0.0 {
            return Ok(north);
        }

        let sw = self.sample_by_offset(cell.base_key, cell.base_side, cell.row0 + 1, cell.col0)?;
        let south = if cell.tx == 0.0 {
            sw
        } else {
            let se =
                self.sample_by_offset(cell.base_key, cell.base_side, cell.row0 + 1, cell.col0 + 1)?;
            sw * (1.0 - cell.tx) + se * cell.tx
        };
        Ok(north * (1.0 - cell.ty) + south * cell.ty)
    }

    fn bicubic_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, TerrainError> {
        let cell = self.interpolation_cell(lat_deg, lon_deg)?;

        let mut grid = [[0.0_f64; 4]; 4];
        for (r, row_values) in grid.iter_mut().enumerate() {
            for (c, value) in row_values.iter_mut().enumerate() {
                let row = cell.row0 + r as isize - 1;
                let col = cell.col0 + c as isize - 1;
                *value = self.sample_by_offset(cell.base_key, cell.base_side, row, col)?;
            }
        }
        Ok(bicubic_unit(cell.tx, cell.ty, grid))
    }

    fn interpolation_cell(
        &self,
        lat_deg: f64,
        lon_deg: f64,
    ) -> Result<InterpolationCell, TerrainError> {
        let (lat_deg, lon_deg) = normalize_query(lat_deg, lon_deg)?;
        let key = tile_key(lat_deg, lon_deg);
        let tile = self.get_or_load_tile(key)?;
        let (x, y) = tile_coordinates(lat_deg, lon_deg, key, tile.side);

        let col0 = x.floor().clamp(0.0, (tile.side - 1) as f64) as usize;
        let row0 = y.floor().clamp(0.0, (tile.side - 1) as f64) as usize;
        let tx = (x - col0 as f64).clamp(0.0, 1.0);
        let ty = (y - row0 as f64).clamp(0.0, 1.0);

        Ok(InterpolationCell {
            base_key: key,
            base_side: tile.side,
            row0: row0 as isize,
            col0: col0 as isize,
            tx,
            ty,
        })
    }

    fn sample_by_offset(
        &self,
        base_key: TileKey,
        base_side: usize,
        row: isize,
        col: isize,
    ) -> Result<f64, TerrainError> {
        let mut key = base_key;
        let mut row = row;
        let mut col = col;
        let stride = (base_side - 1) as isize;
        let max_index = base_side as isize - 1;

        while row < 0 {
            key.lat_floor_deg += 1;
            row += stride;
        }
        while row > max_index {
            key.lat_floor_deg -= 1;
            row -= stride;
        }
        while col < 0 {
            key.lon_floor_deg -= 1;
            col += stride;
        }
        while col > max_index {
            key.lon_floor_deg += 1;
            col -= stride;
        }
        key.lon_floor_deg = normalize_tile_lon_floor(key.lon_floor_deg);

        let tile = self.get_or_load_tile(key)?;
        let row = row.clamp(0, tile.side as isize - 1) as usize;
        let col = col.clamp(0, tile.side as isize - 1) as usize;
        self.resolve_sample_value(&tile, row, col)
    }

    fn resolve_sample_value(
        &self,
        tile: &HgtTile,
        row: usize,
        col: usize,
    ) -> Result<f64, TerrainError> {
        let raw = tile.sample_raw(row, col);
        if raw != VOID_SAMPLE {
            return Ok(raw as f64);
        }

        match self.void_policy {
            VoidPolicy::Error => Err(TerrainError::VoidSample {
                tile_lat_floor_deg: tile.key.lat_floor_deg,
                tile_lon_floor_deg: tile.key.lon_floor_deg,
                row,
                col,
            }),
            VoidPolicy::Zero => Ok(0.0),
            VoidPolicy::NearestValid { max_radius_cells } => {
                self.resolve_void_nearest_in_tile(tile, row, col, max_radius_cells)
            }
        }
    }

    fn resolve_void_nearest_in_tile(
        &self,
        tile: &HgtTile,
        row: usize,
        col: usize,
        max_radius_cells: usize,
    ) -> Result<f64, TerrainError> {
        for radius in 1..=max_radius_cells {
            let row_min = row.saturating_sub(radius);
            let row_max = (row + radius).min(tile.side - 1);
            let col_min = col.saturating_sub(radius);
            let col_max = (col + radius).min(tile.side - 1);

            for candidate_row in row_min..=row_max {
                for candidate_col in col_min..=col_max {
                    let row_delta = candidate_row.abs_diff(row);
                    let col_delta = candidate_col.abs_diff(col);
                    if row_delta.max(col_delta) != radius {
                        continue;
                    }

                    let raw = tile.sample_raw(candidate_row, candidate_col);
                    if raw != VOID_SAMPLE {
                        return Ok(raw as f64);
                    }
                }
            }
        }

        Err(TerrainError::VoidSampleNoNeighbor {
            tile_lat_floor_deg: tile.key.lat_floor_deg,
            tile_lon_floor_deg: tile.key.lon_floor_deg,
            row,
            col,
            max_radius_cells,
        })
    }

    fn get_or_load_tile(&self, key: TileKey) -> Result<Arc<HgtTile>, TerrainError> {
        if let Some(tile) = self.cache_lock_mut()?.get(key) {
            return Ok(tile);
        }

        let path = self.root.join(key.filename());
        if !path.exists() {
            return Err(TerrainError::TileNotFound { path });
        }
        let loaded_tile = Arc::new(HgtTile::from_file(&path, key)?);

        let mut cache = self.cache_lock_mut()?;
        if let Some(existing) = cache.get(key) {
            return Ok(existing);
        }
        cache.insert(key, loaded_tile.clone());
        self.loaded_tile_count.fetch_add(1, Ordering::Relaxed);
        Ok(loaded_tile)
    }

    fn cache_lock_mut(&self) -> Result<std::sync::MutexGuard<'_, CacheState>, TerrainError> {
        self.cache
            .lock()
            .map_err(|_| TerrainError::CacheLockPoisoned)
    }
}

fn normalize_query(lat_deg: f64, lon_deg: f64) -> Result<(f64, f64), TerrainError> {
    if !lat_deg.is_finite() || !(-90.0..=90.0).contains(&lat_deg) {
        return Err(TerrainError::InvalidLatitude(lat_deg));
    }
    if !lon_deg.is_finite() {
        return Err(TerrainError::InvalidLongitude(lon_deg));
    }

    let lat_deg = if lat_deg == 90.0 {
        90.0 - QUERY_EDGE_EPSILON
    } else {
        lat_deg
    };
    let mut lon_deg = (lon_deg + 180.0).rem_euclid(360.0) - 180.0;
    if lon_deg == 180.0 {
        lon_deg = -180.0;
    }

    Ok((lat_deg, lon_deg))
}

fn tile_key(lat_deg: f64, lon_deg: f64) -> TileKey {
    TileKey {
        lat_floor_deg: lat_deg.floor() as i32,
        lon_floor_deg: lon_deg.floor() as i32,
    }
}

fn normalize_tile_lon_floor(lon_floor_deg: i32) -> i32 {
    ((lon_floor_deg + 180).rem_euclid(360)) - 180
}

fn tile_coordinates(lat_deg: f64, lon_deg: f64, tile_key: TileKey, side: usize) -> (f64, f64) {
    let side_minus_one = (side - 1) as f64;
    let frac_lat = (lat_deg - tile_key.lat_floor_deg as f64).clamp(0.0, 1.0);
    let frac_lon = (lon_deg - tile_key.lon_floor_deg as f64).clamp(0.0, 1.0);
    let x = frac_lon * side_minus_one;
    let y = (1.0 - frac_lat) * side_minus_one;
    (x, y)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::thread;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use super::{Interpolation, SrtmDataset, TerrainError, TileKey, VoidPolicy};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "small_world_{label}_{}_{}",
            std::process::id(),
            now
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_tile(path: &Path, tile: TileKey, side: usize, samples: &[i16]) {
        assert_eq!(samples.len(), side * side);
        let tile_path = path.join(tile.filename());
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for value in samples {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        fs::write(tile_path, bytes).unwrap();
    }

    #[test]
    fn bilinear_interpolation_matches_expected_value() {
        let dir = unique_temp_dir("bilinear");
        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        // Row-major, north to south.
        let samples = [
            60, 70, 80, //
            40, 50, 60, //
            20, 30, 40, //
        ];
        write_tile(&dir, tile, 3, &samples);
        let dataset = SrtmDataset::new(&dir);

        let nearest = dataset.elevation_msl(0.1, 0.1).unwrap();
        assert!((nearest - 20.0).abs() < 1e-12);

        let bilinear = dataset.elevation_msl_bilinear(0.25, 0.25).unwrap();
        assert!((bilinear - 35.0).abs() < 1e-12);
    }

    #[test]
    fn bilinear_on_tile_edge_does_not_require_unused_neighbor_tile() {
        let dir = unique_temp_dir("edge_bilinear");
        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        write_tile(&dir, tile, 2, &[10, 20, 30, 40]);

        let dataset = SrtmDataset::new(&dir);
        // Exactly on the south tile edge (lat=0.0): ty==0, south neighbor should not be required.
        let bilinear = dataset.elevation_msl_bilinear(0.0, 0.25).unwrap();
        assert!((bilinear - 32.5).abs() < 1e-12);
    }

    #[test]
    fn bicubic_keeps_constant_surface_constant() {
        let dir = unique_temp_dir("bicubic");
        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        let samples = vec![123_i16; 25];
        write_tile(&dir, tile, 5, &samples);
        let dataset = SrtmDataset::new(&dir);

        let result = dataset
            .elevation_msl_with_interpolation(0.33, 0.42, Interpolation::Bicubic)
            .unwrap();
        assert!((result - 123.0).abs() < 1e-12);
    }

    #[test]
    fn bicubic_uses_neighbor_tile_values_near_edges() {
        let base_tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        let north_tile = TileKey {
            lat_floor_deg: 1,
            lon_floor_deg: 0,
        };
        let base_samples = vec![100_i16; 25];

        let dir_a = unique_temp_dir("bicubic_neighbor_a");
        write_tile(&dir_a, base_tile, 5, &base_samples);
        write_tile(&dir_a, north_tile, 5, &[0_i16; 25]);
        let dataset_a = SrtmDataset::new(&dir_a);
        let result_a = dataset_a.elevation_msl_bicubic(0.95, 0.5).unwrap();

        let dir_b = unique_temp_dir("bicubic_neighbor_b");
        write_tile(&dir_b, base_tile, 5, &base_samples);
        write_tile(&dir_b, north_tile, 5, &[1000_i16; 25]);
        let dataset_b = SrtmDataset::new(&dir_b);
        let result_b = dataset_b.elevation_msl_bicubic(0.95, 0.5).unwrap();

        assert!(
            (result_a - result_b).abs() > 20.0,
            "expected strong bicubic sensitivity to neighbor tile values near boundary, got result_a={result_a}, result_b={result_b}"
        );
    }

    #[test]
    fn longitude_is_normalized() {
        let dir = unique_temp_dir("lon_norm");
        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        let samples = [
            60, 70, 80, //
            40, 50, 60, //
            20, 30, 40, //
        ];
        write_tile(&dir, tile, 3, &samples);
        let dataset = SrtmDataset::new(&dir);

        let a = dataset.elevation_msl_bilinear(0.25, 0.25).unwrap();
        let b = dataset.elevation_msl_bilinear(0.25, 360.25).unwrap();
        assert!((a - b).abs() < 1e-12);
    }

    #[test]
    fn cache_loads_each_tile_once() {
        let dir = unique_temp_dir("cache");
        let tile_0 = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        let tile_1 = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 1,
        };
        write_tile(&dir, tile_0, 2, &[10, 10, 10, 10]);
        write_tile(&dir, tile_1, 2, &[20, 20, 20, 20]);

        let dataset = SrtmDataset::new(&dir);
        for _ in 0..20 {
            let value = dataset.elevation_msl_bilinear(0.2, 0.2).unwrap();
            assert!((value - 10.0).abs() < 1e-12);
        }
        assert_eq!(dataset.loaded_tile_count(), 1);
        assert_eq!(dataset.cached_tile_count(), 1);

        let value = dataset.elevation_msl_bilinear(0.2, 1.2).unwrap();
        assert!((value - 20.0).abs() < 1e-12);
        assert_eq!(dataset.loaded_tile_count(), 2);
        assert_eq!(dataset.cached_tile_count(), 2);
    }

    #[test]
    fn cache_eviction_respects_capacity() {
        let dir = unique_temp_dir("cache_capacity");
        let t0 = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        let t1 = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 1,
        };
        let t2 = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 2,
        };
        write_tile(&dir, t0, 2, &[10, 10, 10, 10]);
        write_tile(&dir, t1, 2, &[20, 20, 20, 20]);
        write_tile(&dir, t2, 2, &[30, 30, 30, 30]);

        let dataset = SrtmDataset::new(&dir).with_max_cached_tiles(2);
        let _ = dataset.elevation_msl(0.2, 0.2).unwrap();
        let _ = dataset.elevation_msl(0.2, 1.2).unwrap();
        let _ = dataset.elevation_msl(0.2, 2.2).unwrap();
        assert_eq!(dataset.cache_capacity(), 2);
        assert_eq!(dataset.cached_tile_count(), 2);
        assert_eq!(dataset.loaded_tile_count(), 3);
    }

    #[test]
    fn reports_missing_tiles_and_invalid_files() {
        let dir = unique_temp_dir("errors");
        let dataset = SrtmDataset::new(&dir);
        let missing = dataset.elevation_msl(0.0, 0.0).unwrap_err();
        assert!(matches!(missing, TerrainError::TileNotFound { .. }));

        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        let tile_path = dir.join(tile.filename());
        fs::write(tile_path, [1_u8, 2_u8, 3_u8]).unwrap();
        let invalid = dataset.elevation_msl(0.0, 0.0).unwrap_err();
        assert!(matches!(invalid, TerrainError::InvalidTileSize { .. }));
    }

    #[test]
    fn detects_void_samples_when_policy_is_error() {
        let dir = unique_temp_dir("void_error");
        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        write_tile(&dir, tile, 2, &[10, 10, -32768, 10]);
        let dataset = SrtmDataset::new(&dir);
        let err = dataset.elevation_msl(0.0, 0.0).unwrap_err();
        assert!(matches!(err, TerrainError::VoidSample { .. }));
    }

    #[test]
    fn zero_void_policy_replaces_voids() {
        let dir = unique_temp_dir("void_zero");
        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        write_tile(&dir, tile, 2, &[10, 10, -32768, 10]);
        let dataset = SrtmDataset::new(&dir).with_void_policy(VoidPolicy::Zero);
        let value = dataset.elevation_msl(0.0, 0.0).unwrap();
        assert_eq!(value, 0.0);
    }

    #[test]
    fn nearest_valid_void_policy_falls_back_to_neighbor() {
        let dir = unique_temp_dir("void_neighbor");
        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        // Query point (0.0, 0.0) resolves to row=1,col=0.
        write_tile(&dir, tile, 2, &[10, 11, -32768, 13]);
        let dataset = SrtmDataset::new(&dir).with_void_policy(VoidPolicy::NearestValid {
            max_radius_cells: 1,
        });
        let value = dataset.elevation_msl(0.0, 0.0).unwrap();
        assert_eq!(value, 10.0);
    }

    #[test]
    fn nearest_valid_void_policy_can_fail_when_radius_is_too_small() {
        let dir = unique_temp_dir("void_neighbor_fail");
        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        write_tile(&dir, tile, 3, &[-32768; 9]);
        let dataset = SrtmDataset::new(&dir).with_void_policy(VoidPolicy::NearestValid {
            max_radius_cells: 2,
        });
        let err = dataset.elevation_msl(0.5, 0.5).unwrap_err();
        assert!(matches!(err, TerrainError::VoidSampleNoNeighbor { .. }));
    }

    #[test]
    fn concurrent_queries_are_safe() {
        let dir = unique_temp_dir("concurrency");
        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        let side = 101;
        let mut samples = vec![0_i16; side * side];
        for row in 0..side {
            for col in 0..side {
                samples[row * side + col] = (row + col) as i16;
            }
        }
        write_tile(&dir, tile, side, &samples);
        let dataset = Arc::new(SrtmDataset::new(&dir));

        let mut handles = Vec::new();
        for thread_id in 0..8 {
            let dataset = Arc::clone(&dataset);
            handles.push(thread::spawn(move || {
                let mut checksum = 0.0;
                for i in 0..5_000 {
                    let lat = 0.01 + ((thread_id * 137 + i) % 950) as f64 / 1000.0;
                    let lon = 0.01 + ((thread_id * 233 + i) % 950) as f64 / 1000.0;
                    checksum += dataset.elevation_msl_bilinear(lat, lon).unwrap();
                }
                checksum
            }));
        }

        let total: f64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
        assert!(total.is_finite());
        assert_eq!(dataset.cached_tile_count(), 1);
        assert_eq!(dataset.loaded_tile_count(), 1);
    }

    #[test]
    fn high_volume_queries_reuse_cache() {
        let dir = unique_temp_dir("perf");
        let tile = TileKey {
            lat_floor_deg: 0,
            lon_floor_deg: 0,
        };
        let side = 301;
        let mut samples = vec![0_i16; side * side];
        for row in 0..side {
            for col in 0..side {
                samples[row * side + col] = (row as i16) + (col as i16);
            }
        }
        write_tile(&dir, tile, side, &samples);

        let dataset = SrtmDataset::new(&dir);
        let start = Instant::now();
        let mut checksum = 0.0_f64;
        for i in 0..100_000 {
            let lat = 0.01 + (i % 900) as f64 / 1000.0;
            let lon = 0.01 + (i % 900) as f64 / 1000.0;
            checksum += dataset.elevation_msl_bilinear(lat, lon).unwrap();
        }
        let elapsed = start.elapsed();

        assert!(checksum.is_finite());
        assert_eq!(dataset.loaded_tile_count(), 1);
        assert_eq!(dataset.cached_tile_count(), 1);
        assert!(
            elapsed.as_secs_f64() < 10.0,
            "high-volume query pass took too long: {:?}",
            elapsed
        );
    }
}
