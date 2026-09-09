use crate::adaptive::normalize_cells_cpu;
use crate::params::AsciiParams;
use serde::Serialize;

#[cfg(feature = "cuda")]
pub mod cuda;
pub mod directcompute;
#[cfg(feature = "opencl")]
pub mod opencl;
#[cfg(feature = "webgpu")]
pub mod webgpu;

pub fn normalize_cells(
    means: &[f32],
    lows: &[f32],
    highs: &[f32],
    params: &AsciiParams,
) -> Vec<f32> {
    #[cfg(feature = "cuda")]
    if params.compute_backend == "cuda" {
        if let Ok(values) = cuda::normalize_cells_cuda(means, lows, highs, params) {
            return values;
        }
    }

    #[cfg(feature = "webgpu")]
    if params.compute_backend == "cuda" || params.compute_backend == "webgpu" {
        if let Ok(values) = webgpu::normalize_cells_webgpu(means, lows, highs, params) {
            return values;
        }
    }

    normalize_cells_cpu(means, lows, highs, params)
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
            executes_on_gpu: false,
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
        label: "DirectCompute (Direct3D 11)".into(),
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
