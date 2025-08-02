use small_world::egm96::EGM96;
use std::path::Path;
use std::env;
use std::time::Instant;

fn main() {
    env::set_var("RUST_BACKTRACE", "1");
    let path = Path::new("data/WW15MGH.DAC");

    
    let mut offset: f32 = 0.0;
    match EGM96::new(path) {
        Ok(egm96) => {
            // println!("EGM96 model loaded successfully!");
            let start = Instant::now();
            // offset = egm96.offset_bilinear(38.6281550, 269.7791550);
            offset = egm96.offset_bicubic(-0.4667440, 0.0023000);
            let duration = start.elapsed();
            println!("Time taken to load EGM96 model: {:?}, with offset {}", duration, offset);
            // println!("Geoid height at (0, 0): {:.2} meters", egm96.read_geoid_value(0, 0).unwrap());
            // println!("Geoid offset at (7.9322759878,	-72.2528133628) = {}", egm96.offset(7.9322759878,	-72.2528133628));
            // println!("Geoid offset at (38.6281550, 269.7791550) = {}. Should be -31.628", egm96.offset_bilinear(38.6281550, 269.7791550));
            // println!("Geoid offset at (-14.6212170, 305.0211140) = {}. Should be -2.969", egm96.offset_bilinear(-14.6212170, 305.0211140));
            // println!("Geoid offset at (46.8743190, 102.4487290) = {}. Should be -43.575", egm96.offset_bilinear(46.8743190, 102.4487290));
            // println!("Geoid offset at (-23.6174460, 133.8747120) = {}. Should be 15.871", egm96.offset_bilinear(-23.6174460, 133.8747120));
            // println!("Geoid offset at (38.6254730, 359.9995000) = {}. Should be 50.066", egm96.offset_bilinear(38.6254730, 359.9995000));
            // println!("Geoid offset at (-0.4667440, 0.0023000) = {}. Should be 17.329", egm96.offset_bilinear(-0.4667440, 0.0023000));

            // println!("Geoid offset at (38.6281550, 269.7791550) = {}. Should be -31.628", egm96.offset_bilinear2(38.6281550, 269.7791550));
            // println!("Geoid offset at (-14.6212170, 305.0211140) = {}. Should be -2.969", egm96.offset_bilinear2(-14.6212170, 305.0211140));
            // println!("Geoid offset at (46.8743190, 102.4487290) = {}. Should be -43.575", egm96.offset_bilinear2(46.8743190, 102.4487290));
            // println!("Geoid offset at (-23.6174460, 133.8747120) = {}. Should be 15.871", egm96.offset_bilinear2(-23.6174460, 133.8747120));
            // println!("Geoid offset at (38.6254730, 359.9995000) = {}. Should be 50.066", egm96.offset_bilinear2(38.6254730, 359.9995000));
            // println!("Geoid offset at (-0.4667440, 0.0023000) = {}. Should be 17.329", egm96.offset_bilinear2(-0.4667440, 0.0023000));

            // println!("Geoid offset at (38.6281550, 269.7791550) = {}. Should be -31.628", egm96.offset_bicubic(38.6281550, 269.7791550));
            // println!("Geoid offset at (-14.6212170, 305.0211140) = {}. Should be -2.969", egm96.offset_bicubic(-14.6212170, 305.0211140));
            // println!("Geoid offset at (46.8743190, 102.4487290) = {}. Should be -43.575", egm96.offset_bicubic(46.8743190, 102.4487290));
            // println!("Geoid offset at (-23.6174460, 133.8747120) = {}. Should be 15.871", egm96.offset_bicubic(-23.6174460, 133.8747120));
            // println!("Geoid offset at (38.6254730, 359.9995000) = {}. Should be 50.066", egm96.offset_bicubic(38.6254730, 359.9995000));
            // println!("Geoid offset at (-0.4667440, 0.0023000) = {}. Should be 17.329", egm96.offset_bicubic(-0.4667440, 0.0023000));
        }
        Err(e) => {
            eprintln!("Failed to load EGM96 model: {}", e);
        }
    }
}
