extern "C" __global__ void region_histogram(
    const unsigned char* luma,
    unsigned int width,
    unsigned int height,
    unsigned int tiles_x,
    unsigned int tiles_y,
    unsigned int* histograms
) {
    unsigned int x = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int y = blockIdx.y * blockDim.y + threadIdx.y;
    if (x >= width || y >= height) return;

    unsigned int tile_x = (unsigned int)((unsigned long long)x * tiles_x / width);
    if (tile_x >= tiles_x) tile_x = tiles_x - 1;
    unsigned int tile_y = (unsigned int)((unsigned long long)y * tiles_y / height);
    if (tile_y >= tiles_y) tile_y = tiles_y - 1;

    unsigned int region_index = tile_y * tiles_x + tile_x;
    unsigned char value = luma[y * width + x];
    atomicAdd(&histograms[region_index * 256u + (unsigned int)value], 1u);
}

extern "C" __global__ void cell_sums(
    const unsigned char* luma,
    unsigned int width,
    unsigned int height,
    unsigned int columns,
    unsigned int rows,
    unsigned int* sums,
    unsigned int* counts
) {
    unsigned int x = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int y = blockIdx.y * blockDim.y + threadIdx.y;
    if (x >= width || y >= height) return;

    unsigned int col = (unsigned int)((unsigned long long)x * columns / width);
    if (col >= columns) col = columns - 1;
    unsigned int row = (unsigned int)((unsigned long long)y * rows / height);
    if (row >= rows) row = rows - 1;

    unsigned int cell_index = row * columns + col;
    unsigned char value = luma[y * width + x];
    atomicAdd(&sums[cell_index], (unsigned int)value);
    atomicAdd(&counts[cell_index], 1u);
}
