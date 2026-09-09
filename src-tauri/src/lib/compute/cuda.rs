use crate::params::AsciiParams;
use cudarc::driver::{CudaContext, LaunchConfig, PushKernelArg};

// Та же формула, что и `adaptive::apply_brightness_contrast`/`normalize_and_adjust`
// — при изменении логики на CPU обновите и это ядро.
const KERNEL_SOURCE: &str = r#"
extern "C" __global__ void normalize_cells(
    const float* means,
    const float* lows,
    const float* highs,
    float* out_values,
    unsigned int count,
    int source_brightness,
    int source_contrast,
    int source_invert,
    int source_black_white,
    int source_threshold,
    int brightness,
    int contrast,
    int invert
) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= count) return;

    float low = lows[i];
    float high = highs[i];
    float range = high - low;
    if (range < 1.0f) range = 1.0f;

    float structural = (means[i] - low) / range;
    structural = fminf(fmaxf(structural, 0.0f), 1.0f);

    float value = (structural - 0.5f) * (1.0f + (float)source_contrast / 100.0f)
        + 0.5f + (float)source_brightness / 100.0f;
    value = fminf(fmaxf(value, 0.0f), 1.0f);
    if (source_invert) value = 1.0f - value;
    if (source_black_white && source_threshold > 0) {
        value = (value * 100.0f < (float)source_threshold) ? 0.0f : 1.0f;
    }

    float final_value = (value - 0.5f) * (1.0f + (float)contrast / 100.0f)
        + 0.5f + (float)brightness / 100.0f;
    final_value = fminf(fmaxf(final_value, 0.0f), 1.0f);
    if (invert) final_value = 1.0f - final_value;

    out_values[i] = final_value;
}
"#;

/// Реально пытается поднять контекст CUDA на устройстве 0. Не паникует,
/// если драйвера/GPU нет — просто `false`.
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

    let d_means = stream.clone_htod(means).map_err(|error| error.to_string())?;
    let d_lows = stream.clone_htod(lows).map_err(|error| error.to_string())?;
    let d_highs = stream.clone_htod(highs).map_err(|error| error.to_string())?;
    let mut d_out = stream
        .alloc_zeros::<f32>(count)
        .map_err(|error| error.to_string())?;

    let ptx = cudarc::nvrtc::compile_ptx(KERNEL_SOURCE).map_err(|error| error.to_string())?;
    let module = ctx.load_module(ptx).map_err(|error| error.to_string())?;
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