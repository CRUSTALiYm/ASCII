extern "C" __global__ void normalize_cells(
    const float* means,
    const float* lows,
    const float* highs,
    float* out_values,
    unsigned int count,
    int source_brightness,
    int source_contrast,
    int source_invert,
    int source_black_white,
    int source_threshold,
    int brightness,
    int contrast,
    int invert
) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= count) return;

    float low = lows[i];
    float high = highs[i];
    float range = high - low;
    if (range < 1.0f) range = 1.0f;

    float structural = (means[i] - low) / range;
    structural = fminf(fmaxf(structural, 0.0f), 1.0f);

    float value = (structural - 0.5f) * (1.0f + (float)source_contrast / 100.0f)
        + 0.5f + (float)source_brightness / 100.0f;
    value = fminf(fmaxf(value, 0.0f), 1.0f);
    if (source_invert) value = 1.0f - value;
    if (source_black_white && source_threshold > 0) {
        value = (value * 100.0f < (float)source_threshold) ? 0.0f : 1.0f;
    }

    float final_value = (value - 0.5f) * (1.0f + (float)contrast / 100.0f)
        + 0.5f + (float)brightness / 100.0f;
    final_value = fminf(fmaxf(final_value, 0.0f), 1.0f);
    if (invert) final_value = 1.0f - final_value;

    out_values[i] = final_value;
}
