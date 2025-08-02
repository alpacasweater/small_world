use std::f32;
use std::fs::File;
use std::cell::RefCell;
use std::io::{self, Seek, SeekFrom};
use std::path::Path;
use byteorder::{BigEndian, ReadBytesExt};
use crate::interpolate::bicubic;

const EGM96_ROWS: usize = 721;  // Latitudes bins
const EGM96_COLS: usize = 1440; // Longitude bins

// TODO: Add bilinear and bicubic interpolation grids that can perist across uses
// caching could make things fast for a lot of repeated calls in a small area.
pub struct EGM96 {
    data_file: RefCell<File>,
    pub geoid: Vec<f32>,
    lat_step_deg: f32,
    lon_step_deg: f32,
}

impl EGM96 {
    pub fn new(path: &Path) -> io::Result<Self> {
        println!("Attempting to construct geoid model");
        let file = File::open(&path)?;
        

        // Generate latitude and longitude arrays
        let lat_step_deg = 180.0 / (EGM96_ROWS as f32 - 1.0);
        let lon_step_deg = 360.0 / (EGM96_COLS as f32);

        Ok(EGM96 {
            data_file: RefCell::new(file),
            geoid: Vec::new(),
            lat_step_deg,
            lon_step_deg
        })
    }

    // This function can be used to load the entire geoid data into memory if needed.
    // For now, we will read values on demand in `read_geoid_value`.
    pub fn load_data(&mut self) -> io::Result<()> {

        let mut file = self.data_file.borrow_mut();

        let mut geoid =vec![0.0f32; EGM96_ROWS * EGM96_COLS];
        for row in 0..EGM96_ROWS {
            for col in 0..EGM96_COLS {
                let raw = file.read_u16::<BigEndian>()?; // Read as big-endian u16
                let signed = raw as i16;                 // Convert to i16 (two's complement)
                let index = EGM96::index(row, col);
                geoid[index] = signed as f32 / 100.0; // Convert to meters
            }
        }
        self.geoid = geoid;

        Ok(())
    }

    // This function allows reading only the geoid data for a specific row and column without loading the entire geoid matrix into memory.
    pub fn read_geoid_value(&self, row: usize, col: usize) -> std::io::Result<f32> {
        let index = EGM96::index(row, col);
        let byte_offset = index * 2; // each value is 2 bytes

        let mut file = self.data_file.borrow_mut();
        file.seek(SeekFrom::Start(byte_offset as u64))?;

        let raw = file.read_u16::<BigEndian>()?;
        let signed = raw as i16; // Offset in centimeters

        Ok(signed as f32 / 100.0) // Return the offset in meters
    }

    // Latitudes in [-90, 90] degrees. Longitudes in [0, 360) degrees.
    pub fn offset(&self, lat: f32, lon: f32) -> f32 {
        // Find the cell of the matrix that corresponds to the input lat, lon
        let eval_lon: f32 = if lon < 0.0 {lon + 360.0} else{lon};
        let lb_row: usize = ((-lat + 90.0)/self.lat_step_deg).floor() as usize;
        let lb_col: usize = ((eval_lon)/self.lon_step_deg).floor() as usize;
        self.read_geoid_value(lb_row, lb_col).unwrap()
        
        // TODO Implement bilinear and bicubic interpolation for more accurate results
    }

    // Latitudes in [-90, 90] degrees. Longitudes in [0, 360) degrees.
    // Supposing the input lat, lon lies within a cell of the egm96 grid
    // returns the indices of the northmost row (latitude), and the eastmost column (longitude)
    pub fn lower_indices(&self, lat: f32, lon: f32) -> (usize, usize) {
        if lat < -90.0 || lat > 90.0 || lon < 0.0 || lon >= 360.0 {
            panic!("Latitude or longitude out of bounds: ({}, {})", lat, lon);
        }
        // Find the cell of the matrix that corresponds to the input lat, lon
        let eval_lon: f32 = if lon < 0.0 {lon + 360.0} else{lon};
        let lb_row: usize = ((-lat + 90.0)/self.lat_step_deg).floor() as usize;
        let lb_col: usize = ((eval_lon)/self.lon_step_deg).floor() as usize;
        if lb_row >= EGM96_ROWS || lb_col >= EGM96_COLS {
            panic!("Indices out of bounds: ({}, {})", lb_row, lb_col);
        }
        (lb_row, lb_col)
    }

    // Latitudes in [-90, 90] degrees. Longitudes in [0, 360) degrees.
    pub fn upper_indices(&self, lat: f32, lon: f32) -> (usize, usize) {
        if lat < -90.0 || lat > 90.0 || lon < 0.0 || lon >= 360.0 {
            panic!("Latitude or longitude out of bounds: ({}, {})", lat, lon);
        }
        let (lb_row, lb_col) = self.lower_indices(lat, lon);
        let ub_row = lb_row + 1;
        let ub_col = (lb_col + 1) % EGM96_COLS; // Wrap around for longitude
        if ub_row >= EGM96_ROWS || ub_col >= EGM96_COLS {
            panic!("Indices out of bounds: ({}, {})", lb_row, lb_col);
        }
        (lb_row, lb_col)
    }

    fn index(row: usize, col: usize) -> usize {
        if row >= EGM96_ROWS || col >= EGM96_COLS {
            panic!("Index out of bounds: ({}, {})", row, col);
        }
        row * EGM96_COLS + col
    }

    // Returns an nxn grid of geoid values centered around the given lat, lon, and associated lat, lon pairs for evaluation.
    // Note that the lat, lon pairs are relative to the input lat, lon and therefore may not adhere to the expected range
    // Latitudes in [-90, 90] degrees. Longitudes in [0, 360) degrees.
    fn get_grid(&self, lat: f32, lon: f32, size: usize) -> (Vec<Vec<f32>>, Vec<Vec<(f32, f32)>>) {
        let mut offset_grid = vec![vec![0.0; size]; size];
        let mut eval_grid = vec![vec![(0.0, 0.0); size]; size];
        if size == 0 {
            panic!("Grid size cannot be zero");
        }

        let half_size = size as f32 / 2.0;
        let lat_lower = lat - half_size * self.lat_step_deg;
        let lon_lower = lon - half_size * self.lon_step_deg;

        for (r, sample_lat) in (0..size).map(|i| lat_lower + i as f32 * self.lat_step_deg).enumerate() {
            for (c, sample_lon) in (0..size).map(|j| lon_lower + j as f32 * self.lon_step_deg).enumerate() {
                let mut eval_lat = sample_lat;
                let mut eval_lon = sample_lon;
                if eval_lon < 0.0 {
                    eval_lon = ((eval_lon % 360.0) + 360.0) % 360.0; // Normalize longitude to [0, 360)
                }
                if eval_lon >= 360.0 {
                    eval_lon = eval_lon % 360.0; // Normalize longitude to [0, 360)
                }
                if eval_lat <= -90.0 {
                    let diff = -90.0 - eval_lat;
                    eval_lat = -90.0 + diff;
                    // TODO: Wrapping around the poles is not handled here for longitude
                }
                if eval_lat > 90.0 {
                    let diff = eval_lat - 90.0;
                    eval_lat = 90.0 - diff;
                    // TODO: Wrapping around the poles is not handled here for longitude
                }
                let (ub_row, ub_col) = self.upper_indices(eval_lat, eval_lon);
                offset_grid[r][c] = self.read_geoid_value(ub_row, ub_col).expect("Failed to read geoid value");
                eval_grid[r][c] = (sample_lat, sample_lon);
            }
        }
        (offset_grid, eval_grid)
    }


    pub fn offset_bilinear(&self, lat: f32, lon: f32) -> f32 {
        let (offset_grid, eval_grid) = self.get_grid(lat, lon, 2);

        // Get the boundaries of the evaluation cell
        let lon_l = eval_grid[0][0].1;
        let lon_u = eval_grid[0][1].1;
        let lat_l = eval_grid[0][0].0;
        let lat_u = eval_grid[1][0].0;

        
        // Interpolate along longitude first
        let fxy1 = offset_grid[0][0]*(lon_u - lon)/self.lon_step_deg + offset_grid[0][1]*(lon - lon_l)/self.lon_step_deg;
        let fxy2 = offset_grid[1][0]*(lon_u - lon)/self.lon_step_deg + offset_grid[1][1]*(lon - lon_l)/self.lon_step_deg;
        
        // Interpolate along latitude
        let fxy = fxy1*(lat_u - lat)/self.lat_step_deg + fxy2*(lat - lat_l)/self.lat_step_deg;
        fxy
    }

    pub fn offset_bilinear2(&self, lat: f32, lon: f32) -> f32 {
        let (offset_grid, eval_grid) = self.get_grid(lat, lon, 2);

        // Get the boundaries of the evaluation cell
        let lat_l = eval_grid[0][0].0;
        let lat_u = eval_grid[1][0].0;
        let lon_l = eval_grid[0][0].1;
        let lon_u = eval_grid[0][1].1;

        // Interpolate along latitude first
        let fxy1 = offset_grid[0][0]*(lat_u - lat)/self.lat_step_deg + offset_grid[0][1]*(lat - lat_l)/self.lat_step_deg;
        let fxy2 = offset_grid[1][0]*(lat_u - lat)/self.lat_step_deg + offset_grid[1][1]*(lat - lat_l)/self.lat_step_deg;
        
        // Interpolate along longitude
        let fxy = fxy1*(lon_u - lon)/self.lon_step_deg + fxy2*(lon - lon_l)/self.lon_step_deg;
        fxy
    }

    pub fn offset_bicubic(&self, lat: f32, lon: f32) -> f32 {
        let (offset_grid, eval_grid) = self.get_grid(lat, lon, 4);

        // println!("Lat: {}, Lon: {}", lat, lon);
        // println!("offset_grid: {:?}", offset_grid);
        // println!("eval_grid: {:?}", eval_grid);

        // Convert Vec<Vec<f32>> to [[f32; 4]; 4]
        let mut offset_arr = [[0.0f32; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                offset_arr[i][j] = offset_grid[i][j];
            }
        }

        // Convert Vec<Vec<(f32, f32)>> to [[(f32, f32); 4]; 4]
        let mut eval_arr = [[(0.0f32, 0.0f32); 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                eval_arr[i][j] = eval_grid[i][j];
            }
        }

        // Interpolate using bicubic interpolation
        let fxy = bicubic(lon, lat, offset_arr, eval_arr);
        fxy
    }
}
