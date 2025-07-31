use std::f32;
use std::fs::File;
use std::cell::RefCell;
use std::io::{self, Seek, SeekFrom};
use std::path::Path;
use byteorder::{BigEndian, ReadBytesExt};

const EGM96_ROWS: usize = 721;  // Latitudes bins
const EGM96_COLS: usize = 1440; // Longitude bins

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

    fn index(row: usize, col: usize) -> usize {
        if row >= EGM96_ROWS || col >= EGM96_COLS {
            panic!("Index out of bounds: ({}, {})", row, col);
        }
        row * EGM96_COLS + col
    }

}
