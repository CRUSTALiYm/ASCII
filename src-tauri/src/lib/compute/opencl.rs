/// Реальная проверка через перечисление OpenCL-платформ. Расчёт для
/// OpenCL в движке пока не подключён (см. compute::mod) — только детект.
pub fn is_available() -> bool {
    opencl3::platform::get_platforms()
        .map(|platforms| !platforms.is_empty())
        .unwrap_or(false)
}