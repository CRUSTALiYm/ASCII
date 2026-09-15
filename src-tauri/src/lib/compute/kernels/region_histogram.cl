#pragma OPENCL EXTENSION cl_khr_global_int32_base_atomics : enable

__kernel void region_histogram(
    __global const uchar* luma,
    uint width,
    uint height,
    uint tiles_x,
    uint tiles_y,
    __global uint* histograms
) {
    uint idx = get_global_id(0);
    if (idx >= width * height) return;
    uint x = idx % width;
    uint y = idx / width;

    uint tile_x = (uint)(((ulong)x * tiles_x) / width);
    if (tile_x >= tiles_x) tile_x = tiles_x - 1;
    uint tile_y = (uint)(((ulong)y * tiles_y) / height);
    if (tile_y >= tiles_y) tile_y = tiles_y - 1;

    uint region_index = tile_y * tiles_x + tile_x;
    uchar value = luma[idx];
    atomic_add(&histograms[region_index * 256u + (uint)value], 1u);
}
