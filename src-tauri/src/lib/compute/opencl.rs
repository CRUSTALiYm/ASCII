use crate::compute::PixelScanResult;
use crate::params::AsciiParams;
use opencl3::command_queue::{CommandQueue, CL_QUEUE_PROFILING_ENABLE};
use opencl3::context::Context;
use opencl3::device::{get_all_devices, Device, CL_DEVICE_TYPE_GPU};
use opencl3::kernel::{ExecuteKernel, Kernel};
use opencl3::memory::{Buffer, CL_MEM_READ_ONLY, CL_MEM_READ_WRITE};
use opencl3::program::Program;
use opencl3::types::{CL_BLOCKING, CL_NON_BLOCKING};
use std::ptr;

const REGION_HISTOGRAM_SOURCE: &str = include_str!("kernels/region_histogram.cl");
const CELL_SUMS_SOURCE: &str = include_str!("kernels/cell_sums.cl");
const NORMALIZE_SOURCE: &str = include_str!("kernels/normalize_cells.cl");

pub fn is_available() -> bool {
    opencl3::platform::get_platforms()
        .map(|platforms| !platforms.is_empty())
        .unwrap_or(false)
}

fn first_gpu_context() -> Result<(Context, Device), String> {
    let device_id = *get_all_devices(CL_DEVICE_TYPE_GPU)
        .map_err(|error| error.to_string())?
        .first()
        .ok_or("Нет OpenCL GPU-устройств")?;
    let device = Device::new(device_id);
    let context = Context::from_device(&device).map_err(|error| error.to_string())?;
    Ok((context, device))
}

fn build_program(context: &Context, source: &str) -> Result<Program, String> {
    Program::create_and_build_from_source(context, source, "")
        .map_err(|error| format!("Ошибка сборки OpenCL-ядра: {error}"))
}

pub fn scan_pixels_opencl(
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
    let pixel_count = (width * height) as usize;

    let (context, device) = first_gpu_context()?;
    let queue = unsafe {
        CommandQueue::create_with_properties(&context, device.id(), CL_QUEUE_PROFILING_ENABLE, 0)
    }
    .map_err(|error| error.to_string())?;

    let mut luma_buf =
        unsafe { Buffer::<u8>::create(&context, CL_MEM_READ_ONLY, pixel_count, ptr::null_mut()) }
            .map_err(|error| error.to_string())?;
    unsafe {
        queue
            .enqueue_write_buffer(&mut luma_buf, CL_NON_BLOCKING, 0, luma.as_raw(), &[])
            .map_err(|error| error.to_string())?;
    }

    let histogram_len = (tiles_x * tiles_y * 256) as usize;
    let zero_histograms = vec![0u32; histogram_len];
    let mut histograms_buf = unsafe {
        Buffer::<u32>::create(&context, CL_MEM_READ_WRITE, histogram_len, ptr::null_mut())
    }
    .map_err(|error| error.to_string())?;
    unsafe {
        queue
            .enqueue_write_buffer(&mut histograms_buf, CL_BLOCKING, 0, &zero_histograms, &[])
            .map_err(|error| error.to_string())?;
    }

    let cell_count = (columns * rows) as usize;
    let zero_cells = vec![0u32; cell_count];
    let mut sums_buf =
        unsafe { Buffer::<u32>::create(&context, CL_MEM_READ_WRITE, cell_count, ptr::null_mut()) }
            .map_err(|error| error.to_string())?;
    let mut counts_buf =
        unsafe { Buffer::<u32>::create(&context, CL_MEM_READ_WRITE, cell_count, ptr::null_mut()) }
            .map_err(|error| error.to_string())?;
    unsafe {
        queue
            .enqueue_write_buffer(&mut sums_buf, CL_BLOCKING, 0, &zero_cells, &[])
            .map_err(|error| error.to_string())?;
        queue
            .enqueue_write_buffer(&mut counts_buf, CL_BLOCKING, 0, &zero_cells, &[])
            .map_err(|error| error.to_string())?;
    }

    let histogram_program = build_program(&context, REGION_HISTOGRAM_SOURCE)?;
    let histogram_kernel = Kernel::create(&histogram_program, "region_histogram")
        .map_err(|error| error.to_string())?;
    let histogram_event = unsafe {
        ExecuteKernel::new(&histogram_kernel)
            .set_arg(&luma_buf)
            .set_arg(&width)
            .set_arg(&height)
            .set_arg(&tiles_x)
            .set_arg(&tiles_y)
            .set_arg(&histograms_buf)
            .set_global_work_size(pixel_count)
            .enqueue_nd_range(&queue)
            .map_err(|error| error.to_string())?
    };
    histogram_event.wait().map_err(|error| error.to_string())?;

    let cell_program = build_program(&context, CELL_SUMS_SOURCE)?;
    let cell_kernel =
        Kernel::create(&cell_program, "cell_sums").map_err(|error| error.to_string())?;
    let cell_event = unsafe {
        ExecuteKernel::new(&cell_kernel)
            .set_arg(&luma_buf)
            .set_arg(&width)
            .set_arg(&height)
            .set_arg(&columns)
            .set_arg(&rows)
            .set_arg(&sums_buf)
            .set_arg(&counts_buf)
            .set_global_work_size(pixel_count)
            .enqueue_nd_range(&queue)
            .map_err(|error| error.to_string())?
    };
    cell_event.wait().map_err(|error| error.to_string())?;

    let mut histograms_flat = vec![0u32; histogram_len];
    unsafe {
        queue
            .enqueue_read_buffer(&histograms_buf, CL_BLOCKING, 0, &mut histograms_flat, &[])
            .map_err(|error| error.to_string())?;
    }
    let mut sums = vec![0u32; cell_count];
    let mut counts = vec![0u32; cell_count];
    unsafe {
        queue
            .enqueue_read_buffer(&sums_buf, CL_BLOCKING, 0, &mut sums, &[])
            .map_err(|error| error.to_string())?;
        queue
            .enqueue_read_buffer(&counts_buf, CL_BLOCKING, 0, &mut counts, &[])
            .map_err(|error| error.to_string())?;
    }
    queue.finish().map_err(|error| error.to_string())?;

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

pub fn normalize_cells_opencl(
    means: &[f32],
    lows: &[f32],
    highs: &[f32],
    params: &AsciiParams,
) -> Result<Vec<f32>, String> {
    let count = means.len();
    if count == 0 {
        return Ok(Vec::new());
    }

    let (context, device) = first_gpu_context()?;
    let queue = unsafe {
        CommandQueue::create_with_properties(&context, device.id(), CL_QUEUE_PROFILING_ENABLE, 0)
    }
    .map_err(|error| error.to_string())?;

    let mut means_buf =
        unsafe { Buffer::<f32>::create(&context, CL_MEM_READ_ONLY, count, ptr::null_mut()) }
            .map_err(|error| error.to_string())?;
    let mut lows_buf =
        unsafe { Buffer::<f32>::create(&context, CL_MEM_READ_ONLY, count, ptr::null_mut()) }
            .map_err(|error| error.to_string())?;
    let mut highs_buf =
        unsafe { Buffer::<f32>::create(&context, CL_MEM_READ_ONLY, count, ptr::null_mut()) }
            .map_err(|error| error.to_string())?;
    let out_buf =
        unsafe { Buffer::<f32>::create(&context, CL_MEM_READ_WRITE, count, ptr::null_mut()) }
            .map_err(|error| error.to_string())?;

    unsafe {
        queue
            .enqueue_write_buffer(&mut means_buf, CL_NON_BLOCKING, 0, means, &[])
            .map_err(|error| error.to_string())?;
        queue
            .enqueue_write_buffer(&mut lows_buf, CL_NON_BLOCKING, 0, lows, &[])
            .map_err(|error| error.to_string())?;
        queue
            .enqueue_write_buffer(&mut highs_buf, CL_BLOCKING, 0, highs, &[])
            .map_err(|error| error.to_string())?;
    }

    let program = build_program(&context, NORMALIZE_SOURCE)?;
    let kernel = Kernel::create(&program, "normalize_cells").map_err(|error| error.to_string())?;

    let count_u32 = count as u32;
    let source_invert_flag: i32 = params.source_invert as i32;
    let source_bw_flag: i32 = params.source_black_white as i32;
    let invert_flag: i32 = params.invert as i32;

    let event = unsafe {
        ExecuteKernel::new(&kernel)
            .set_arg(&means_buf)
            .set_arg(&lows_buf)
            .set_arg(&highs_buf)
            .set_arg(&out_buf)
            .set_arg(&count_u32)
            .set_arg(&params.source_brightness)
            .set_arg(&params.source_contrast)
            .set_arg(&source_invert_flag)
            .set_arg(&source_bw_flag)
            .set_arg(&params.source_threshold)
            .set_arg(&params.brightness)
            .set_arg(&params.contrast)
            .set_arg(&invert_flag)
            .set_global_work_size(count)
            .enqueue_nd_range(&queue)
            .map_err(|error| error.to_string())?
    };
    event.wait().map_err(|error| error.to_string())?;

    let mut result = vec![0f32; count];
    unsafe {
        queue
            .enqueue_read_buffer(&out_buf, CL_BLOCKING, 0, &mut result, &[])
            .map_err(|error| error.to_string())?;
    }
    queue.finish().map_err(|error| error.to_string())?;

    Ok(result)
}
