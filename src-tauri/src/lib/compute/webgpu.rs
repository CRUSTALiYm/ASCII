use crate::compute::PixelScanResult;
use crate::params::AsciiParams;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

const NORMALIZE_SHADER_SOURCE: &str = include_str!("kernels/normalize_cells.wgsl");
const REGION_HISTOGRAM_SHADER: &str = include_str!("kernels/region_histogram.wgsl");
const CELL_SUMS_SHADER: &str = include_str!("kernels/cell_sums.wgsl");

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct NormalizeParams {
    count: u32,
    source_brightness: i32,
    source_contrast: i32,
    source_invert: u32,
    source_black_white: u32,
    source_threshold: i32,
    brightness: i32,
    contrast: i32,
    invert: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct HistogramDims {
    width: u32,
    height: u32,
    tiles_x: u32,
    tiles_y: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CellDims {
    width: u32,
    height: u32,
    columns: u32,
    rows: u32,
}

async fn request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await
        .ok()?;
    adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .ok()
}

pub fn is_available() -> bool {
    pollster::block_on(request_device()).is_some()
}

fn poll_and_read<T: Pod>(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<T>, String> {
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| error.to_string())?;
    receiver
        .recv()
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;

    let data = slice.get_mapped_range().map_err(|error| error.to_string())?;
    let result: Vec<T> = bytemuck::cast_slice(&data).to_vec();
    drop(data);
    buffer.unmap();
    Ok(result)
}

fn staging_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

pub fn scan_pixels_webgpu(
    luma: &image::GrayImage,
    columns: u32,
    rows: u32,
    tiles_x: u32,
    tiles_y: u32,
) -> Result<PixelScanResult, String> {
    pollster::block_on(run_scan(luma, columns, rows, tiles_x, tiles_y))
}

async fn run_scan(
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

    let (device, queue) = request_device().await.ok_or("WebGPU-адаптер не найден")?;

    // Storage-буферы WGSL оперируют u32 — каждый байт яркости кладём в
    // отдельный u32. Проще и безопаснее упаковки 4xu8 в один u32, хоть и
    // не самое компактное представление в памяти.
    let luma_u32: Vec<u32> = luma.as_raw().iter().map(|&b| b as u32).collect();
    let luma_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("luma"),
        contents: bytemuck::cast_slice(&luma_u32),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let histogram_len = (tiles_x * tiles_y * 256) as u64;
    let histograms_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("histograms"),
        size: histogram_len * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let cell_count = (columns * rows) as u64;
    let sums_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sums"),
        size: cell_count * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let counts_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("counts"),
        size: cell_count * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let histogram_dims_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("histogram_dims"),
        contents: bytemuck::bytes_of(&HistogramDims {
            width,
            height,
            tiles_x,
            tiles_y,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let cell_dims_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cell_dims"),
        contents: bytemuck::bytes_of(&CellDims {
            width,
            height,
            columns,
            rows,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let histogram_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("region_histogram"),
        source: wgpu::ShaderSource::Wgsl(REGION_HISTOGRAM_SHADER.into()),
    });
    let histogram_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("region_histogram_pipeline"),
        layout: None,
        module: &histogram_shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let histogram_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("region_histogram_bind_group"),
        layout: &histogram_pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: luma_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: histograms_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: histogram_dims_buf.as_entire_binding(),
            },
        ],
    });

    let cell_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cell_sums"),
        source: wgpu::ShaderSource::Wgsl(CELL_SUMS_SHADER.into()),
    });
    let cell_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("cell_sums_pipeline"),
        layout: None,
        module: &cell_shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let cell_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cell_sums_bind_group"),
        layout: &cell_pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: luma_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: sums_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: counts_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: cell_dims_buf.as_entire_binding(),
            },
        ],
    });

    let workgroups_x = (width + 15) / 16;
    let workgroups_y = (height + 15) / 16;

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&histogram_pipeline);
        pass.set_bind_group(0, &histogram_bind_group, &[]);
        pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
    }
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&cell_pipeline);
        pass.set_bind_group(0, &cell_bind_group, &[]);
        pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
    }

    let histograms_staging = staging_buffer(&device, "histograms_staging", histogram_len * 4);
    let sums_staging = staging_buffer(&device, "sums_staging", cell_count * 4);
    let counts_staging = staging_buffer(&device, "counts_staging", cell_count * 4);

    encoder.copy_buffer_to_buffer(&histograms_buf, 0, &histograms_staging, 0, histogram_len * 4);
    encoder.copy_buffer_to_buffer(&sums_buf, 0, &sums_staging, 0, cell_count * 4);
    encoder.copy_buffer_to_buffer(&counts_buf, 0, &counts_staging, 0, cell_count * 4);
    queue.submit(Some(encoder.finish()));

    let histograms_flat: Vec<u32> = poll_and_read(&device, &histograms_staging)?;
    let sums: Vec<u32> = poll_and_read(&device, &sums_staging)?;
    let counts: Vec<u32> = poll_and_read(&device, &counts_staging)?;

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
        .map(|(&sum, &count)| if count > 0 { sum as f32 / count as f32 } else { 0.0 })
        .collect();

    Ok(PixelScanResult {
        region_histograms,
        cell_means,
    })
}

pub fn normalize_cells_webgpu(
    means: &[f32],
    lows: &[f32],
    highs: &[f32],
    params: &AsciiParams,
) -> Result<Vec<f32>, String> {
    pollster::block_on(run_normalize(means, lows, highs, params))
}

async fn run_normalize(
    means: &[f32],
    lows: &[f32],
    highs: &[f32],
    params: &AsciiParams,
) -> Result<Vec<f32>, String> {
    let count = means.len();
    if count == 0 {
        return Ok(Vec::new());
    }

    let (device, queue) = request_device().await.ok_or("WebGPU-адаптер не найден")?;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("normalize_cells"),
        source: wgpu::ShaderSource::Wgsl(NORMALIZE_SHADER_SOURCE.into()),
    });

    let means_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("means"),
        contents: bytemuck::cast_slice(means),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let lows_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("lows"),
        contents: bytemuck::cast_slice(lows),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let highs_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("highs"),
        contents: bytemuck::cast_slice(highs),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let out_size = (count * std::mem::size_of::<f32>()) as u64;
    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("out"),
        size: out_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let staging_buf = staging_buffer(&device, "staging", out_size);

    let gpu_params = NormalizeParams {
        count: count as u32,
        source_brightness: params.source_brightness,
        source_contrast: params.source_contrast,
        source_invert: params.source_invert as u32,
        source_black_white: params.source_black_white as u32,
        source_threshold: params.source_threshold,
        brightness: params.brightness,
        contrast: params.contrast,
        invert: params.invert as u32,
        pad0: 0,
        pad1: 0,
        pad2: 0,
    };
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("params"),
        contents: bytemuck::bytes_of(&gpu_params),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("normalize_cells_pipeline"),
        layout: None,
        module: &shader,
        entry_point: Some("normalize_cells"),
        compilation_options: Default::default(),
        cache: None,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("normalize_cells_bind_group"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: means_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: lows_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: highs_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: out_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: params_buf.as_entire_binding(),
            },
        ],
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        let workgroups = (count as u32 + 255) / 256;
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&out_buf, 0, &staging_buf, 0, out_size);
    queue.submit(Some(encoder.finish()));

    poll_and_read(&device, &staging_buf)
}