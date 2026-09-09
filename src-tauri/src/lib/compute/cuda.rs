use crate::params::AsciiParams;
use cudarc::driver::{CudaContext, LaunchConfig, PushKernelArg};

const KERNEL_CU_SOURCE: &str = include_str!("kernels/normalize_cells.cu");

const PRECOMPILED_PTX: &str = include_str!(concat!(env!("OUT_DIR"), "/normalize_cells.ptx"));

pub fn is_available() -> bool {
    CudaContext::new(0).is_ok()
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

    let module = if PRECOMPILED_PTX.trim().is_empty() {
        let ptx =
            cudarc::nvrtc::compile_ptx(KERNEL_CU_SOURCE).map_err(|error| error.to_string())?;
        ctx.load_module(ptx).map_err(|error| error.to_string())?
    } else {
        let ptx = cudarc::nvrtc::Ptx::from_src(PRECOMPILED_PTX);
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
