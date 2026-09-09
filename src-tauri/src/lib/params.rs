use serde::{Deserialize, Serialize};

/// Параметры одного преобразования кадра в ASCII.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AsciiParams {
    pub job_id: u64,
    pub columns: u32,
    pub characters: String,

    pub brightness: i32,
    pub contrast: i32,
    pub invert: bool,

    pub source_brightness: i32,
    pub source_contrast: i32,
    pub source_invert: bool,
    pub source_black_white: bool,
    pub source_threshold: i32,

    #[serde(default = "default_true")]
    pub adaptive: bool,
    /// Размер региональной сетки (крупнее ASCII-сетки), из которой клетки
    /// берут диапазон нормализации.
    #[serde(default = "default_region_tiles")]
    pub adaptive_region_tiles: u32,
    /// Обрезка с каждого края гистограммы региона, в процентах.
    #[serde(default = "default_percentile")]
    pub adaptive_percentile: f32,
    /// 0..1, насколько диапазон региона подтягивается к соседям.
    #[serde(default = "default_smoothing")]
    pub adaptive_smoothing: f32,
    /// Минимальная ширина диапазона нормализации (0..255).
    #[serde(default = "default_min_range")]
    pub adaptive_min_range: f32,
    /// Порог финального подавления выбросов (0..1).
    #[serde(default = "default_despike_threshold")]
    pub adaptive_despike_threshold: f32,
    /// "cpu" | "cuda" | "opencl" | "directcompute".
    #[serde(default = "default_compute_backend")]
    pub compute_backend: String,
}

fn default_true() -> bool {
    true
}
fn default_region_tiles() -> u32 {
    8
}
fn default_percentile() -> f32 {
    5.0
}
fn default_smoothing() -> f32 {
    0.6
}
fn default_min_range() -> f32 {
    30.0
}
fn default_despike_threshold() -> f32 {
    0.12
}
fn default_compute_backend() -> String {
    "cpu".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertRequest {
    pub path: String,
    #[serde(flatten)]
    pub params: AsciiParams,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameConvertRequest {
    pub frame_base64: String,
    #[serde(flatten)]
    pub params: AsciiParams,
}

#[derive(Debug, Serialize)]
pub struct ConvertResult {
    pub columns: u32,
    pub rows: u32,
    pub values: Vec<String>,
    pub text: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ConversionProgress {
    #[serde(rename = "jobId")]
    pub job_id: u64,
    pub processed: usize,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct BestSettings {
    pub resolution: u32,
}

#[cfg(test)]
pub fn test_params() -> AsciiParams {
    AsciiParams {
        job_id: 1,
        columns: 40,
        characters: " .:-=+*#%@".to_string(),
        brightness: 0,
        contrast: 0,
        invert: false,
        source_brightness: 0,
        source_contrast: 0,
        source_invert: false,
        source_black_white: false,
        source_threshold: 0,
        adaptive: true,
        adaptive_region_tiles: default_region_tiles(),
        adaptive_percentile: default_percentile(),
        adaptive_smoothing: default_smoothing(),
        adaptive_min_range: default_min_range(),
        adaptive_despike_threshold: default_despike_threshold(),
        compute_backend: default_compute_backend(),
    }
}