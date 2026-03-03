#![no_main]

use std::fs;

use libfuzzer_sys::fuzz_target;
use small_world::terrain::SrtmDataset;
use tempfile::tempdir;

const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 || data.len() > MAX_INPUT_BYTES {
        return;
    }

    let lat_seed = u16::from_le_bytes([data[0], data[1]]);
    let lon_seed = u16::from_le_bytes([data[2], data[3]]);
    let lat = f64::from(lat_seed % 10_000) / 10_000.0;
    let lon = f64::from(lon_seed % 10_000) / 10_000.0 + if data[0] & 1 == 1 { 360.0 } else { 0.0 };

    let payload = &data[4..];
    let dir = match tempdir() {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let tile_path = dir.path().join("N00E000.hgt");
    if fs::write(&tile_path, payload).is_err() {
        return;
    }

    let dataset = SrtmDataset::new(dir.path());
    let _ = dataset.elevation_msl(lat, lon);
    let _ = dataset.elevation_msl_bilinear(lat, lon);
    let _ = dataset.elevation_msl_bicubic(lat, lon);
    let _ = dataset.elevation_msl_bilinear(lat + 1.0, lon);
    let _ = dataset.elevation_msl_bilinear(lat, lon + 1.0);
});
