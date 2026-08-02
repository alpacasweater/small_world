#![no_main]

use std::fs;

use libfuzzer_sys::fuzz_target;
use small_world::geoid::{EgmModel, EGM2008, EGM96};
use tempfile::tempdir;

const MAX_INPUT_BYTES: usize = 3 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 || data.len() > MAX_INPUT_BYTES {
        return;
    }

    let lat_seed = i16::from_le_bytes([data[0], data[1]]);
    let lon_seed = i16::from_le_bytes([data[2], data[3]]);
    let lat = (lat_seed as f64 / 200.0).clamp(-90.0, 90.0);
    let lon = lon_seed as f64;

    let dir = match tempdir() {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let path = dir.path().join(EgmModel::Egm96.canonical_filename());
    if fs::write(&path, data).is_err() {
        return;
    }

    if let Ok(mut egm96) = EGM96::new(&path) {
        let _ = egm96.offset(lat, lon);
        let _ = egm96.offset_bilinear(lat, lon);
        let _ = egm96.offset_bicubic(lat, lon);
        let _ = egm96.lower_indices(lat, lon);
        let _ = egm96.upper_indices(lat, lon);

        let _ = egm96.load_data();
        let _ = egm96.offset(lat, lon);
        let _ = egm96.offset_bilinear(lat, lon);
        let _ = egm96.offset_bicubic(lat, lon);
    }

    let _ = EGM2008::new(&path);
});
