/// Проверяет, доступна ли поддержка DirectCompute.
/// Возвращает `true`, если целевая операционная система — Windows.
pub fn is_available() -> bool {
    cfg!(target_os = "windows")
}