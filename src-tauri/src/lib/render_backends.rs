use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RenderBackendInfo {
    id: String,
    label: String,
    kind: String, // "cpu" | "gpu"
    is_default: bool,
}

/// Определяет реально доступные бэкенды рендера
#[cfg(feature = "gpu-backends")]
#[tauri::command]
pub fn list_render_backends() -> Vec<RenderBackendInfo> {
    let mut backends = vec![RenderBackendInfo {
        id: "cpu".into(),
        label: "CPU (универсальный, всегда доступен)".into(),
        kind: "cpu".into(),
        is_default: false,
    }];

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

    for adapter in pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all())) {
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            continue;
        }
        let backend_id = match info.backend {
            wgpu::Backend::Vulkan => "vulkan",
            wgpu::Backend::Dx12 => "dx12",
            wgpu::Backend::Metal => "metal",
            wgpu::Backend::Gl => "gl",
            _ => continue,
        };
        backends.push(RenderBackendInfo {
            id: format!("{backend_id}:{}", info.device),
            label: format!("{} ({})", info.name, backend_label(info.backend)),
            kind: "gpu".into(),
            is_default: false,
        });
    }

    if let Some(first_gpu) = backends.iter_mut().find(|backend| backend.kind == "gpu") {
        first_gpu.is_default = true;
    } else if let Some(cpu) = backends.first_mut() {
        cpu.is_default = true;
    }

    backends
}

#[cfg(feature = "gpu-backends")]
fn backend_label(backend: wgpu::Backend) -> &'static str {
    match backend {
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Dx12 => "DirectX 12",
        wgpu::Backend::Metal => "Metal",
        wgpu::Backend::Gl => "OpenGL",
        _ => "Неизвестный backend",
    }
}

#[cfg(not(feature = "gpu-backends"))]
#[tauri::command]
pub fn list_render_backends() -> Vec<RenderBackendInfo> {
    vec![RenderBackendInfo {
        id: "cpu".into(),
        label: "CPU (универсальный, всегда доступен)".into(),
        kind: "cpu".into(),
        is_default: true,
    }]
}