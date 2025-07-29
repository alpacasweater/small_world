use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use byteorder::{BigEndian, ReadBytesExt};

const EGM96_ROWS: usize = 721;  // Latitudes bins
const EGM96_COLS: usize = 1440; // Longitude bins

pub struct EGM96 {
    pub geoid: Box<[[f32; EGM96_COLS]; EGM96_ROWS]>,
    lat_step_deg: f32,
    lon_step_deg: f32,
}

impl EGM96 {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut geoid = Box::new([[0.0f32; EGM96_COLS]; EGM96_ROWS]);

        for row in 0..EGM96_ROWS {
            for col in 0..EGM96_COLS {
                let raw = file.read_u16::<BigEndian>()?; // Read as big-endian u16
                let signed = raw as i16;                 // Convert to i16 (two's complement)
                geoid[row][col] = signed as f32 / 100.0; // Convert to meters
            }
        }

        // Generate latitude and longitude arrays
        let lat_step_deg = 180.0 / (EGM96_ROWS as f32 - 1.0);
        let lon_step_deg = 360.0 / (EGM96_COLS as f32);

        Ok(EGM96 {
            geoid,
            lat_step_deg,
            lon_step_deg
        })
    }

    // Latitudes in [-90, 90] degrees. Longitudes in [0, 360) degrees.
    pub fn offset(self, lat: &f32, lon: &f32) -> f32 {
        println!("(lat, lon): ({}, {})", lat, lon);
        // Find the cell of the matrix that corresponds to the input lat, lon
        let eval_lon: f32 = if *lon < 0.0f32 {lon + 360.0f32} else{*lon};
        let lb_row: usize = ((-lat + 90.0f32)/self.lat_step_deg).floor() as usize;
        let lb_col: usize = ((eval_lon)/self.lon_step_deg).floor() as usize;
        println!("(row, col): ({}, {})", lb_row, lb_col);
        println!("(offset: {}", self.geoid[lb_row][lb_col]);
        return 10.0f32;
        // return Interp2(lb_row, lb_col, self.geoid, lat, lon, (double) -32767);
    }

}
