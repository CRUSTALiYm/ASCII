use crate::params::AsciiParams;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

const SHADER_SOURCE: &str = include_str!("kernels/normalize_cells.wgsl");

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GpuParams {
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

pub fn normalize_cells_webgpu(
    means: &[f32],
    lows: &[f32],
    highs: &[f32],
    params: &AsciiParams,
) -> Result<Vec<f32>, String> {
    pollster::block_on(run(means, lows, highs, params))
}

async fn run(
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
        source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
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
    let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("staging"),
        size: out_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let gpu_params = GpuParams {
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

    let bind_group_layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("normalize_cells_bind_group"),
        layout: &bind_group_layout,
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

    let slice = staging_buf.slice(..);
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

    let data = slice
        .get_mapped_range()
        .map_err(|error| error.to_string())?;
    let result: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
    drop(data);
    staging_buf.unmap();

    Ok(result)
}
