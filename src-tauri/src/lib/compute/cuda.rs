use crate::compute::PixelScanResult;
use crate::params::AsciiParams;
use cudarc::driver::{CudaContext, LaunchConfig, PushKernelArg};

const NORMALIZE_KERNEL_CU_SOURCE: &str = include_str!("kernels/normalize_cells.cu");
const NORMALIZE_PRECOMPILED_PTX: &str =
    include_str!(concat!(env!("OUT_DIR"), "/normalize_cells.ptx"));
const SCAN_KERNEL_SOURCE: &str = include_str!("kernels/scan_pixels.cu");
const SCAN_PRECOMPILED_PTX: &str = include_str!(concat!(env!("OUT_DIR"), "/scan_pixels.ptx"));

pub fn is_available() -> bool {
    CudaContext::new(0).is_ok()
}

pub fn scan_pixels_cuda(
    luma: &image::GrayImage,
    columns: u32,
    rows: u32,
    tiles_x: u32,
    tiles_y: u32,
) -> Result<PixelScanResult, String> {
    let (width, height) = luma.dimensions();
    if width == 0 || height == 0 {
        return Err("Пустое изображение".into());
    }

    let ctx = CudaContext::new(0).map_err(|error| error.to_string())?;
    let stream = ctx.default_stream();

    let d_luma = stream
        .clone_htod(luma.as_raw())
        .map_err(|error| error.to_string())?;

    let histogram_len = (tiles_x * tiles_y * 256) as usize;
    let mut d_histograms = stream
        .alloc_zeros::<u32>(histogram_len)
        .map_err(|error| error.to_string())?;

    let cell_count = (columns * rows) as usize;
    let mut d_sums = stream
        .alloc_zeros::<u32>(cell_count)
        .map_err(|error| error.to_string())?;
    let mut d_counts = stream
        .alloc_zeros::<u32>(cell_count)
        .map_err(|error| error.to_string())?;

    let ptx = if SCAN_PRECOMPILED_PTX.trim().is_empty() {
        cudarc::nvrtc::compile_ptx(SCAN_KERNEL_SOURCE).map_err(|error| error.to_string())?
    } else {
        cudarc::nvrtc::Ptx::from_src(SCAN_PRECOMPILED_PTX)
    };
    let module = ctx.load_module(ptx).map_err(|error| error.to_string())?;
    let histogram_kernel = module
        .load_function("region_histogram")
        .map_err(|error| error.to_string())?;
    let means_kernel = module
        .load_function("cell_sums")
        .map_err(|error| error.to_string())?;

    let block = (16u32, 16u32, 1u32);
    let grid = ((width + 15) / 16, (height + 15) / 16, 1u32);
    let launch_config = LaunchConfig {
        grid_dim: grid,
        block_dim: block,
        shared_mem_bytes: 0,
    };

    let mut builder = stream.launch_builder(&histogram_kernel);
    builder.arg(&d_luma);
    builder.arg(&width);
    builder.arg(&height);
    builder.arg(&tiles_x);
    builder.arg(&tiles_y);
    builder.arg(&mut d_histograms);
    unsafe { builder.launch(launch_config) }.map_err(|error| error.to_string())?;

    let mut builder = stream.launch_builder(&means_kernel);
    builder.arg(&d_luma);
    builder.arg(&width);
    builder.arg(&height);
    builder.arg(&columns);
    builder.arg(&rows);
    builder.arg(&mut d_sums);
    builder.arg(&mut d_counts);
    unsafe { builder.launch(launch_config) }.map_err(|error| error.to_string())?;

    let histograms_flat: Vec<u32> = stream
        .clone_dtoh(&d_histograms)
        .map_err(|error| error.to_string())?;
    let sums: Vec<u32> = stream
        .clone_dtoh(&d_sums)
        .map_err(|error| error.to_string())?;
    let counts: Vec<u32> = stream
        .clone_dtoh(&d_counts)
        .map_err(|error| error.to_string())?;

    let region_histograms = histograms_flat
        .chunks_exact(256)
        .map(|chunk| {
            let mut histogram = [0u32; 256];
            histogram.copy_from_slice(chunk);
            histogram
        })
        .collect();

    let cell_means = sums
        .iter()
        .zip(counts.iter())
        .map(|(&sum, &count)| {
            if count > 0 {
                sum as f32 / count as f32
            } else {
                0.0
            }
        })
        .collect();

    Ok(PixelScanResult {
        region_histograms,
        cell_means,
    })
}

pub fn normalize_cells_cuda(
    means: &[f32],
    lows: &[f32],
    highs: &[f32],
    params: &AsciiParams,
) -> Result<Vec<f32>, String> {
    let count = means.len();
    if count == 0 {
        return Ok(Vec::new());
    }

    let ctx = CudaContext::new(0).map_err(|error| error.to_string())?;
    let stream = ctx.default_stream();

    let d_means = stream
        .clone_htod(means)
        .map_err(|error| error.to_string())?;
    let d_lows = stream.clone_htod(lows).map_err(|error| error.to_string())?;
    let d_highs = stream
        .clone_htod(highs)
        .map_err(|error| error.to_string())?;
    let mut d_out = stream
        .alloc_zeros::<f32>(count)
        .map_err(|error| error.to_string())?;

    let module = if NORMALIZE_PRECOMPILED_PTX.trim().is_empty() {
        let ptx = cudarc::nvrtc::compile_ptx(NORMALIZE_KERNEL_CU_SOURCE)
            .map_err(|error| error.to_string())?;
        ctx.load_module(ptx).map_err(|error| error.to_string())?
    } else {
        let ptx = cudarc::nvrtc::Ptx::from_src(NORMALIZE_PRECOMPILED_PTX);
        ctx.load_module(ptx).map_err(|error| error.to_string())?
    };

    let kernel = module
        .load_function("normalize_cells")
        .map_err(|error| error.to_string())?;

    let count_u32 = count as u32;
    let source_invert_flag: i32 = params.source_invert as i32;
    let source_bw_flag: i32 = params.source_black_white as i32;
    let invert_flag: i32 = params.invert as i32;

    let mut builder = stream.launch_builder(&kernel);
    builder.arg(&d_means);
    builder.arg(&d_lows);
    builder.arg(&d_highs);
    builder.arg(&mut d_out);
    builder.arg(&count_u32);
    builder.arg(&params.source_brightness);
    builder.arg(&params.source_contrast);
    builder.arg(&source_invert_flag);
    builder.arg(&source_bw_flag);
    builder.arg(&params.source_threshold);
    builder.arg(&params.brightness);
    builder.arg(&params.contrast);
    builder.arg(&invert_flag);

    unsafe { builder.launch(LaunchConfig::for_num_elems(count_u32)) }
        .map_err(|error| error.to_string())?;

    stream.clone_dtoh(&d_out).map_err(|error| error.to_string())
}
