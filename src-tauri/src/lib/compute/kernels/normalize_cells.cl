__kernel void normalize_cells(
    __global const float* means,
    __global const float* lows,
    __global const float* highs,
    __global float* out_values,
    uint count,
    int source_brightness,
    int source_contrast,
    int source_invert,
    int source_black_white,
    int source_threshold,
    int brightness,
    int contrast,
    int invert
) {
    uint i = get_global_id(0);
    if (i >= count) return;

    float low = lows[i];
    float high = highs[i];
    float range = high - low;
    if (range < 1.0f) range = 1.0f;

    float structural = (means[i] - low) / range;
    structural = clamp(structural, 0.0f, 1.0f);

    float value = (structural - 0.5f) * (1.0f + (float)source_contrast / 100.0f)
        + 0.5f + (float)source_brightness / 100.0f;
    value = clamp(value, 0.0f, 1.0f);
    if (source_invert) value = 1.0f - value;
    if (source_black_white && source_threshold > 0) {
        value = (value * 100.0f < (float)source_threshold) ? 0.0f : 1.0f;
    }

    float final_value = (value - 0.5f) * (1.0f + (float)contrast / 100.0f)
        + 0.5f + (float)brightness / 100.0f;
    final_value = clamp(final_value, 0.0f, 1.0f);
    if (invert) final_value = 1.0f - final_value;

    out_values[i] = final_value;
}
