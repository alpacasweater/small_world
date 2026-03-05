use std::env;
use std::ffi::CString;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::egm96::EGM96;
use small_world::ffi::{
    sw_converter_convert_height_m, sw_converter_create, sw_converter_destroy,
    sw_converter_options_default, SwConverterHandle, SwConverterOptions, SwStatus, SwVerticalFrame,
};
use small_world::height::Interpolation;
use small_world::terrain::SrtmDataset;
use small_world::wgs84::{AltType, Lla, Ned};

#[derive(Clone, Copy)]
struct Metric {
    iterations: u64,
    elapsed_s: f64,
    ops_per_sec: f64,
    ns_per_op: f64,
    p50_ns_per_op: f64,
    p95_ns_per_op: f64,
    p99_ns_per_op: f64,
}

struct PerfSummary {
    altitude_dataset: Metric,
    terrain_bilinear: Metric,
    wgs84_round_trip: Metric,
    ffi_single_thread: Metric,
    ffi_shared_handle_8t: Metric,
    ffi_per_thread_handles_8t: Metric,
    ffi_threads: usize,
    max_rss_kb: Option<u64>,
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let p = percentile.clamp(0.0, 1.0);
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx]
}

fn metric_from_samples(iterations: u64, elapsed_s: f64, samples_ns_per_op: &[f64]) -> Metric {
    let safe_elapsed_s = elapsed_s.max(1e-12);
    Metric {
        iterations,
        elapsed_s: safe_elapsed_s,
        ops_per_sec: iterations as f64 / safe_elapsed_s,
        ns_per_op: safe_elapsed_s * 1e9 / iterations.max(1) as f64,
        p50_ns_per_op: percentile(samples_ns_per_op, 0.50),
        p95_ns_per_op: percentile(samples_ns_per_op, 0.95),
        p99_ns_per_op: percentile(samples_ns_per_op, 0.99),
    }
}

fn measure(iterations: u64, sample_stride: u64, mut f: impl FnMut(u64) -> f64) -> Metric {
    let stride = sample_stride.max(1);
    let mut samples_ns_per_op = Vec::new();
    let mut checksum = 0.0_f64;

    let mut i = 0_u64;
    let total_start = Instant::now();
    while i < iterations {
        let chunk = (iterations - i).min(stride);
        let chunk_start = Instant::now();
        for _ in 0..chunk {
            checksum += f(i);
            i += 1;
        }
        let chunk_elapsed_s = chunk_start.elapsed().as_secs_f64().max(1e-12);
        samples_ns_per_op.push(chunk_elapsed_s * 1e9 / chunk as f64);
    }
    black_box(checksum);

    let elapsed_s = total_start.elapsed().as_secs_f64();
    metric_from_samples(iterations, elapsed_s, &samples_ns_per_op)
}

fn metric_json(name: &str, metric: Metric) -> String {
    format!(
        "    \"{name}\": {{\"iterations\": {}, \"elapsed_s\": {:.9}, \"ops_per_sec\": {:.3}, \"ns_per_op\": {:.3}, \"p50_ns_per_op\": {:.3}, \"p95_ns_per_op\": {:.3}, \"p99_ns_per_op\": {:.3}}}",
        metric.iterations,
        metric.elapsed_s,
        metric.ops_per_sec,
        metric.ns_per_op,
        metric.p50_ns_per_op,
        metric.p95_ns_per_op,
        metric.p99_ns_per_op,
    )
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = env::temp_dir().join(format!(
        "small_world_perf_{label}_{}_{}",
        std::process::id(),
        now
    ));
    fs::create_dir_all(&dir).expect("failed to create temporary benchmark directory");
    dir
}

fn perf_thread_count() -> usize {
    if let Ok(raw) = env::var("SMALL_WORLD_PERF_THREADS") {
        if let Ok(parsed) = raw.parse::<usize>() {
            return parsed.clamp(1, 8);
        }
    }
    thread::available_parallelism()
        .map(|n| n.get().clamp(1, 8))
        .unwrap_or(4)
}

fn write_linear_hgt_tile(root: &Path, side: usize) {
    let tile_path = root.join("N00E000.hgt");
    let mut bytes = Vec::with_capacity(side * side * 2);
    for row in 0..side {
        for col in 0..side {
            let value = (1000 + (row * 2 + col * 3) as i32) as i16;
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }
    fs::write(tile_path, bytes).expect("failed to write synthetic HGT tile");
}

fn egm96_path() -> PathBuf {
    if let Ok(path) = env::var("SMALL_WORLD_PERF_EGM96") {
        return PathBuf::from(path);
    }
    PathBuf::from("data/WW15MGH.DAC")
}

fn bench_altitude_conversion_dataset(
    geoid_path: &Path,
    terrain_root: &Path,
) -> Result<Metric, String> {
    let mut geoid =
        EGM96::new(geoid_path).map_err(|err| format!("failed to open geoid dataset: {err}"))?;
    geoid
        .load_data()
        .map_err(|err| format!("failed to preload geoid dataset: {err}"))?;
    let terrain = SrtmDataset::new(terrain_root);
    let converter = AltitudeConverter::new(&geoid, &terrain)
        .with_geoid_interpolation(Interpolation::Bilinear)
        .with_terrain_interpolation(Interpolation::Bilinear);

    let metric = measure(400_000, 1_000, |i| {
        let lat = 0.01 + (i % 970) as f64 / 1000.0;
        let lon = 0.01 + ((i * 7) % 970) as f64 / 1000.0;
        let point = GeoPoint::new(lat, lon).expect("valid benchmark point");
        let input_agl_m = 50.0 + (i % 1000) as f64 * 0.01;
        converter
            .convert_height_m(point, input_agl_m, VerticalFrame::Agl, VerticalFrame::Hae)
            .expect("dataset-backed conversion should succeed")
    });
    Ok(metric)
}

fn bench_terrain_bilinear(terrain_root: &Path) -> Metric {
    let dataset = SrtmDataset::new(terrain_root);
    measure(300_000, 1_000, |i| {
        let lat = 0.01 + (i % 970) as f64 / 1000.0;
        let lon = 0.01 + ((i * 7) % 970) as f64 / 1000.0;
        dataset
            .elevation_msl_bilinear(lat, lon)
            .expect("terrain query should succeed")
    })
}

fn bench_wgs84_round_trip() -> Metric {
    let origin = Lla::new(39.0, -77.0, 300.0, AltType::Wgs84);

    measure(1_000_000, 2_000, |i| {
        let ned = Ned::new(
            (i % 2000) as f64 * 0.1 - 100.0,
            ((i * 3) % 2000) as f64 * 0.1 - 100.0,
            ((i * 5) % 400) as f64 * 0.1 - 20.0,
            origin,
        );
        let lla = ned.to_lla();
        let ned_back = Ned::from_lla(lla, origin);
        ned_back.n() + ned_back.e() + ned_back.d()
    })
}

unsafe fn ffi_options_default() -> Result<SwConverterOptions, String> {
    let mut options = SwConverterOptions {
        geoid_model: small_world::ffi::SwGeoidModel::Egm96,
        geoid_interpolation: small_world::ffi::SwInterpolation::Bilinear,
        terrain_interpolation: small_world::ffi::SwInterpolation::Bilinear,
        terrain_cache_tiles: 64,
        void_policy: small_world::ffi::SwVoidPolicy::Error,
        void_policy_radius_cells: 3,
        preload_geoid: 1,
    };
    // SAFETY: points to valid writable memory.
    let status = unsafe { sw_converter_options_default(&mut options) };
    if status != SwStatus::Ok {
        return Err(format!(
            "sw_converter_options_default failed with status {:?}",
            status
        ));
    }
    Ok(options)
}

unsafe fn ffi_create_converter(
    geoid_path: &Path,
    terrain_root: &Path,
) -> Result<*mut SwConverterHandle, String> {
    let geoid_c = CString::new(geoid_path.to_string_lossy().as_bytes())
        .map_err(|_| "geoid path contains interior NUL".to_string())?;
    let terrain_c = CString::new(terrain_root.to_string_lossy().as_bytes())
        .map_err(|_| "terrain root contains interior NUL".to_string())?;
    // SAFETY: helper returns initialized defaults.
    let options = unsafe { ffi_options_default()? };
    let mut handle: *mut SwConverterHandle = std::ptr::null_mut();
    // SAFETY: pointers remain valid for this call; out ptr is valid.
    let status =
        unsafe { sw_converter_create(geoid_c.as_ptr(), terrain_c.as_ptr(), &options, &mut handle) };
    if status != SwStatus::Ok || handle.is_null() {
        return Err(format!(
            "sw_converter_create failed with status {:?}, null_handle={}",
            status,
            handle.is_null()
        ));
    }
    Ok(handle)
}

fn ffi_convert_once(handle: *mut SwConverterHandle, i: u64, thread_id: usize) -> f64 {
    let lat = 0.01 + ((i + thread_id as u64 * 37) % 970) as f64 / 1000.0;
    let lon = 0.01 + (((i * 7) + thread_id as u64 * 19) % 970) as f64 / 1000.0;
    let input_agl_m = 35.0 + (i % 500) as f64 * 0.05;
    let mut out = 0.0;
    // SAFETY: handle and output pointer are valid for this call.
    let status = unsafe {
        sw_converter_convert_height_m(
            handle,
            lat,
            lon,
            input_agl_m,
            SwVerticalFrame::Agl,
            SwVerticalFrame::Hae,
            &mut out,
        )
    };
    if status != SwStatus::Ok {
        panic!(
            "sw_converter_convert_height_m failed with status {:?}",
            status
        );
    }
    out
}

fn bench_ffi_single_thread(geoid_path: &Path, terrain_root: &Path) -> Result<Metric, String> {
    // SAFETY: path inputs are valid and converted to C strings.
    let handle = unsafe { ffi_create_converter(geoid_path, terrain_root)? };
    let metric = measure(250_000, 1_000, |i| ffi_convert_once(handle, i, 0));
    // SAFETY: handle was allocated by sw_converter_create and is still valid.
    unsafe { sw_converter_destroy(handle) };
    Ok(metric)
}

fn parallel_metric(iterations_total: u64, elapsed_s: f64, thread_ns_per_op: &[f64]) -> Metric {
    metric_from_samples(iterations_total, elapsed_s, thread_ns_per_op)
}

fn bench_ffi_shared_handle_8t(
    geoid_path: &Path,
    terrain_root: &Path,
    threads: usize,
) -> Result<Metric, String> {
    let iterations_per_thread = 80_000_u64;
    // SAFETY: path inputs are valid and converted to C strings.
    let handle = unsafe { ffi_create_converter(geoid_path, terrain_root)? };
    let shared = handle as usize;

    let start = Instant::now();
    let mut joins = Vec::with_capacity(threads);
    for thread_id in 0..threads {
        joins.push(thread::spawn(move || {
            let handle = shared as *mut SwConverterHandle;
            let thread_start = Instant::now();
            let mut checksum = 0.0_f64;
            for i in 0..iterations_per_thread {
                checksum += ffi_convert_once(handle, i, thread_id);
            }
            let thread_elapsed_s = thread_start.elapsed().as_secs_f64().max(1e-12);
            let thread_ns_per_op = thread_elapsed_s * 1e9 / iterations_per_thread as f64;
            (checksum, thread_ns_per_op)
        }));
    }

    let mut checksum = 0.0_f64;
    let mut thread_samples = Vec::with_capacity(threads);
    for join in joins {
        let (thread_checksum, thread_ns_per_op) = join
            .join()
            .map_err(|_| "ffi shared-handle benchmark worker thread panicked".to_string())?;
        checksum += thread_checksum;
        thread_samples.push(thread_ns_per_op);
    }
    black_box(checksum);
    let elapsed_s = start.elapsed().as_secs_f64();

    // SAFETY: handle was allocated by sw_converter_create and is still valid.
    unsafe { sw_converter_destroy(handle) };

    Ok(parallel_metric(
        iterations_per_thread * threads as u64,
        elapsed_s,
        &thread_samples,
    ))
}

fn bench_ffi_per_thread_handles_8t(
    geoid_path: &Path,
    terrain_root: &Path,
    threads: usize,
) -> Result<Metric, String> {
    let iterations_per_thread = 80_000_u64;
    let mut handles = Vec::with_capacity(threads);
    for _ in 0..threads {
        // SAFETY: path inputs are valid and converted to C strings.
        let handle = unsafe { ffi_create_converter(geoid_path, terrain_root)? };
        handles.push(handle as usize);
    }

    let start = Instant::now();
    let mut joins = Vec::with_capacity(threads);
    for (thread_id, handle) in handles.iter().copied().enumerate() {
        joins.push(thread::spawn(move || {
            let handle = handle as *mut SwConverterHandle;
            let thread_start = Instant::now();
            let mut checksum = 0.0_f64;
            for i in 0..iterations_per_thread {
                checksum += ffi_convert_once(handle, i, thread_id);
            }
            let thread_elapsed_s = thread_start.elapsed().as_secs_f64().max(1e-12);
            let thread_ns_per_op = thread_elapsed_s * 1e9 / iterations_per_thread as f64;
            (checksum, thread_ns_per_op)
        }));
    }

    let mut checksum = 0.0_f64;
    let mut thread_samples = Vec::with_capacity(threads);
    for join in joins {
        let (thread_checksum, thread_ns_per_op) = join
            .join()
            .map_err(|_| "ffi per-thread benchmark worker panicked".to_string())?;
        checksum += thread_checksum;
        thread_samples.push(thread_ns_per_op);
    }
    black_box(checksum);
    let elapsed_s = start.elapsed().as_secs_f64();

    for handle in handles {
        // SAFETY: handles were allocated by sw_converter_create and are still valid.
        unsafe { sw_converter_destroy(handle as *mut SwConverterHandle) };
    }

    Ok(parallel_metric(
        iterations_per_thread * threads as u64,
        elapsed_s,
        &thread_samples,
    ))
}

#[cfg(unix)]
fn peak_rss_kb() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: usage points to writable memory and RUSAGE_SELF is valid.
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    // SAFETY: getrusage initialized `usage` when rc == 0.
    let usage = unsafe { usage.assume_init() };
    let raw = usage.ru_maxrss as u64;
    #[cfg(target_os = "macos")]
    {
        Some(raw / 1024)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(raw)
    }
}

#[cfg(all(not(unix), not(windows)))]
fn peak_rss_kb() -> Option<u64> {
    None
}

#[cfg(windows)]
fn peak_rss_kb() -> Option<u64> {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};

    type Bool = i32;
    type Dword = u32;
    type Handle = *mut c_void;

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: Dword,
        page_fault_count: Dword,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> Handle;
    }

    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: Handle,
            counters: *mut ProcessMemoryCounters,
            cb: Dword,
        ) -> Bool;
    }

    let mut counters: ProcessMemoryCounters = unsafe { zeroed() };
    counters.cb = size_of::<ProcessMemoryCounters>() as Dword;

    let ok = unsafe {
        // SAFETY: handles and pointers are valid for this process; `cb` matches struct size.
        let handle = GetCurrentProcess();
        GetProcessMemoryInfo(handle, &mut counters, counters.cb)
    };
    if ok == 0 {
        return None;
    }
    Some((counters.peak_working_set_size as u64) / 1024)
}

fn json_string(summary: &PerfSummary) -> String {
    let shared_scale_vs_ideal = summary.ffi_shared_handle_8t.ops_per_sec
        / (summary.ffi_single_thread.ops_per_sec * summary.ffi_threads as f64).max(1e-12);
    let per_thread_scale_vs_ideal = summary.ffi_per_thread_handles_8t.ops_per_sec
        / (summary.ffi_single_thread.ops_per_sec * summary.ffi_threads as f64).max(1e-12);
    let max_rss_json = summary
        .max_rss_kb
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string());

    format!(
        "{{\n\
  \"process\": {{\"max_rss_kb\": {max_rss_json}}},\n\
  \"metrics\": {{\n\
{},\n\
{},\n\
{},\n\
{},\n\
{},\n\
{}\n\
  }},\n\
  \"derived\": {{\n\
    \"ffi_thread_count\": {},\n\
    \"ffi_shared_scale_vs_ideal\": {:.6},\n\
    \"ffi_per_thread_scale_vs_ideal\": {:.6}\n\
  }}\n\
}}\n",
        metric_json("altitude_dataset", summary.altitude_dataset),
        metric_json("terrain_bilinear", summary.terrain_bilinear),
        metric_json("wgs84_round_trip", summary.wgs84_round_trip),
        metric_json("ffi_single_thread", summary.ffi_single_thread),
        metric_json("ffi_shared_handle_8t", summary.ffi_shared_handle_8t),
        metric_json(
            "ffi_per_thread_handles_8t",
            summary.ffi_per_thread_handles_8t
        ),
        summary.ffi_threads,
        shared_scale_vs_ideal,
        per_thread_scale_vs_ideal,
    )
}

fn parse_json_out_arg() -> Option<PathBuf> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--json-out" {
            if let Some(path) = args.next() {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

fn print_metric(label: &str, metric: Metric) {
    println!(
        "  {label}: {:.0} ops/s ({:.1} ns/op, p95 {:.1} ns/op)",
        metric.ops_per_sec, metric.ns_per_op, metric.p95_ns_per_op
    );
}

fn main() -> Result<(), String> {
    let geoid_path = egm96_path();
    if !geoid_path.exists() {
        return Err(format!(
            "required geoid dataset not found: {} (set SMALL_WORLD_PERF_EGM96 to override)",
            geoid_path.display()
        ));
    }

    let terrain_root = unique_temp_dir("terrain");
    write_linear_hgt_tile(&terrain_root, 1201);
    let ffi_threads = perf_thread_count();

    let altitude_dataset = bench_altitude_conversion_dataset(&geoid_path, &terrain_root)?;
    let terrain_bilinear = bench_terrain_bilinear(&terrain_root);
    let wgs84_round_trip = bench_wgs84_round_trip();
    let ffi_single_thread = bench_ffi_single_thread(&geoid_path, &terrain_root)?;
    let ffi_shared_handle_8t = bench_ffi_shared_handle_8t(&geoid_path, &terrain_root, ffi_threads)?;
    let ffi_per_thread_handles_8t =
        bench_ffi_per_thread_handles_8t(&geoid_path, &terrain_root, ffi_threads)?;
    let max_rss_kb = peak_rss_kb();
    let summary = PerfSummary {
        altitude_dataset,
        terrain_bilinear,
        wgs84_round_trip,
        ffi_single_thread,
        ffi_shared_handle_8t,
        ffi_per_thread_handles_8t,
        ffi_threads,
        max_rss_kb,
    };

    println!("small_world perf smoke metrics (dataset-backed):");
    print_metric("altitude_dataset", summary.altitude_dataset);
    print_metric("terrain_bilinear", summary.terrain_bilinear);
    print_metric("wgs84_round_trip", summary.wgs84_round_trip);
    print_metric("ffi_single_thread", summary.ffi_single_thread);
    print_metric("ffi_shared_handle_8t", summary.ffi_shared_handle_8t);
    print_metric(
        "ffi_per_thread_handles_8t",
        summary.ffi_per_thread_handles_8t,
    );

    let shared_scale_vs_ideal = summary.ffi_shared_handle_8t.ops_per_sec
        / (summary.ffi_single_thread.ops_per_sec * summary.ffi_threads as f64).max(1e-12);
    let per_thread_scale_vs_ideal = summary.ffi_per_thread_handles_8t.ops_per_sec
        / (summary.ffi_single_thread.ops_per_sec * summary.ffi_threads as f64).max(1e-12);
    println!("  ffi_thread_count: {}", summary.ffi_threads);
    println!("  ffi_shared_scale_vs_ideal: {:.4}", shared_scale_vs_ideal);
    println!(
        "  ffi_per_thread_scale_vs_ideal: {:.4}",
        per_thread_scale_vs_ideal
    );
    match summary.max_rss_kb {
        Some(value) => println!("  max_rss_kb: {value}"),
        None => println!("  max_rss_kb: unavailable"),
    }

    let json = json_string(&summary);
    if let Some(path) = parse_json_out_arg() {
        fs::write(&path, json)
            .map_err(|err| format!("failed to write perf JSON {}: {err}", path.display()))?;
        println!("  json_output: {}", path.display());
    } else {
        println!("\n{json}");
    }

    Ok(())
}
