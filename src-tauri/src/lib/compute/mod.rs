use crate::adaptive;
use crate::params::AsciiParams;
use serde::Serialize;

#[cfg(feature = "cuda")]
pub mod cuda;
pub mod directcompute;
#[cfg(feature = "opencl")]
pub mod opencl;
#[cfg(feature = "webgpu")]
pub mod webgpu;

pub struct PixelScanResult {
    pub region_histograms: Vec<[u32; 256]>, // tiles_y * tiles_x штук
    pub cell_means: Vec<f32>,               // rows * columns штук
}

/// "auto" — вся цепочка по приоритету.
fn backend_order(requested: &str) -> &'static [&'static str] {
    match requested {
        "auto" => &["cuda", "webgpu", "opencl", "directcompute", "cpu"],
        "cuda" => &["cuda", "cpu"],
        "webgpu" => &["webgpu", "cpu"],
        "opencl" => &["opencl", "cpu"],
        "directcompute" => &["directcompute", "cpu"],
        _ => &["cpu"],
    }
}

pub fn scan_pixels(
    luma: &image::GrayImage,
    columns: u32,
    rows: u32,
    tiles_x: u32,
    tiles_y: u32,
    params: &AsciiParams,
) -> PixelScanResult {
    for backend in backend_order(&params.compute_backend) {
        let attempt: Option<PixelScanResult> = match *backend {
            #[cfg(feature = "cuda")]
            "cuda" => cuda::scan_pixels_cuda(luma, columns, rows, tiles_x, tiles_y).ok(),
            #[cfg(feature = "webgpu")]
            "webgpu" => webgpu::scan_pixels_webgpu(luma, columns, rows, tiles_x, tiles_y).ok(),
            #[cfg(feature = "opencl")]
            "opencl" => opencl::scan_pixels_opencl(luma, columns, rows, tiles_x, tiles_y).ok(),
            "cpu" => Some(adaptive::scan_pixels_cpu(
                luma, columns, rows, tiles_x, tiles_y,
            )),
            _ => None,
        };
        match attempt {
            Some(result) => {
                eprintln!("[compute] scan_pixels: выполнено на {backend}");
                return result;
            }
            None => {
                eprintln!("[compute] scan_pixels: {backend} недоступен/не сработал, пробуем дальше")
            }
        }
    }
    adaptive::scan_pixels_cpu(luma, columns, rows, tiles_x, tiles_y)
}

pub fn normalize_cells(
    means: &[f32],
    lows: &[f32],
    highs: &[f32],
    params: &AsciiParams,
) -> Vec<f32> {
    for backend in backend_order(&params.compute_backend) {
        let attempt: Option<Vec<f32>> = match *backend {
            #[cfg(feature = "cuda")]
            "cuda" => cuda::normalize_cells_cuda(means, lows, highs, params).ok(),
            #[cfg(feature = "webgpu")]
            "webgpu" => webgpu::normalize_cells_webgpu(means, lows, highs, params).ok(),
            #[cfg(feature = "opencl")]
            "opencl" => opencl::normalize_cells_opencl(means, lows, highs, params).ok(),
            "cpu" => Some(adaptive::normalize_cells_cpu(means, lows, highs, params)),
            _ => None,
        };
        match attempt {
            Some(result) => {
                eprintln!("[compute] normalize_cells: выполнено на {backend}");
                return result;
            }
            None => {
                eprintln!(
                    "[compute] normalize_cells: {backend} недоступен/не сработал, пробуем дальше"
                )
            }
        }
    }
    adaptive::normalize_cells_cpu(means, lows, highs, params)
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ComputeBackendInfo {
    id: String,
    label: String,
    is_available: bool,
    executes_on_gpu: bool,
    is_default: bool,
}

/// проверяет, что установлено на машине.
#[tauri::command]
pub fn list_compute_backends() -> Vec<ComputeBackendInfo> {
    let mut backends = vec![ComputeBackendInfo {
        id: "cpu".into(),
        label: "CPU (rayon, всегда доступен)".into(),
        is_available: true,
        executes_on_gpu: false,
        is_default: true,
    }];

    #[cfg(feature = "cuda")]
    {
        let available = cuda::is_available();
        backends.push(ComputeBackendInfo {
            id: "cuda".into(),
            label: "NVIDIA CUDA".into(),
            is_available: available,
            executes_on_gpu: available,
            is_default: false,
        });
    }
    #[cfg(not(feature = "cuda"))]
    backends.push(ComputeBackendInfo {
        id: "cuda".into(),
        label: "NVIDIA CUDA (не собрано в этой сборке)".into(),
        is_available: false,
        executes_on_gpu: false,
        is_default: false,
    });

    #[cfg(feature = "webgpu")]
    {
        let available = webgpu::is_available();
        backends.push(ComputeBackendInfo {
            id: "webgpu".into(),
            label: "WebGPU (Vulkan/DX12/Metal — любой GPU)".into(),
            is_available: available,
            executes_on_gpu: available,
            is_default: false,
        });
    }
    #[cfg(not(feature = "webgpu"))]
    backends.push(ComputeBackendInfo {
        id: "webgpu".into(),
        label: "WebGPU (не собрано в этой сборке)".into(),
        is_available: false,
        executes_on_gpu: false,
        is_default: false,
    });

    #[cfg(feature = "opencl")]
    {
        let available = opencl::is_available();
        backends.push(ComputeBackendInfo {
            id: "opencl".into(),
            label: "OpenCL".into(),
            is_available: available,
            executes_on_gpu: available,
            is_default: false,
        });
    }
    #[cfg(not(feature = "opencl"))]
    backends.push(ComputeBackendInfo {
        id: "opencl".into(),
        label: "OpenCL (не собрано в этой сборке)".into(),
        is_available: false,
        executes_on_gpu: false,
        is_default: false,
    });

    backends.push(ComputeBackendInfo {
        id: "directcompute".into(),
        label: "DirectCompute (Direct3D 11) — только определение, расчёт не подключён".into(),
        is_available: directcompute::is_available(),
        executes_on_gpu: false,
        is_default: false,
    });

    if let Some(best_id) = backends
        .iter()
        .find(|backend| backend.is_available && backend.executes_on_gpu)
        .map(|backend| backend.id.clone())
    {
        for backend in backends.iter_mut() {
            backend.is_default = backend.id == best_id;
        }
    }

    backends
}
