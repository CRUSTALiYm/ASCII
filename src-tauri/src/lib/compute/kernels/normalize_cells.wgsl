struct Params {
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
};

@group(0) @binding(0) var<storage, read> means: array<f32>;
@group(0) @binding(1) var<storage, read> lows: array<f32>;
@group(0) @binding(2) var<storage, read> highs: array<f32>;
@group(0) @binding(3) var<storage, read_write> out_values: array<f32>;
@group(0) @binding(4) var<uniform> params: Params;

@compute @workgroup_size(256)
fn normalize_cells(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.count) {
        return;
    }

    let low = lows[i];
    let high = highs[i];
    var range = high - low;
    if (range < 1.0) {
        range = 1.0;
    }

    var structural = (means[i] - low) / range;
    structural = clamp(structural, 0.0, 1.0);

    var value = (structural - 0.5) * (1.0 + f32(params.source_contrast) / 100.0)
        + 0.5 + f32(params.source_brightness) / 100.0;
    value = clamp(value, 0.0, 1.0);
    if (params.source_invert != 0u) {
        value = 1.0 - value;
    }
    if (params.source_black_white != 0u && params.source_threshold > 0) {
        if (value * 100.0 < f32(params.source_threshold)) {
            value = 0.0;
        } else {
            value = 1.0;
        }
    }

    var final_value = (value - 0.5) * (1.0 + f32(params.contrast) / 100.0)
        + 0.5 + f32(params.brightness) / 100.0;
    final_value = clamp(final_value, 0.0, 1.0);
    if (params.invert != 0u) {
        final_value = 1.0 - final_value;
    }

    out_values[i] = final_value;
}
