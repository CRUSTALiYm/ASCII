#pragma OPENCL EXTENSION cl_khr_global_int32_base_atomics : enable

__kernel void cell_sums(
    __global const uchar* luma,
    uint width,
    uint height,
    uint columns,
    uint rows,
    __global uint* sums,
    __global uint* counts
) {
    uint idx = get_global_id(0);
    if (idx >= width * height) return;
    uint x = idx % width;
    uint y = idx / width;

    uint col = (uint)(((ulong)x * columns) / width);
    if (col >= columns) col = columns - 1;
    uint row = (uint)(((ulong)y * rows) / height);
    if (row >= rows) row = rows - 1;

    uint cell_index = row * columns + col;
    uchar value = luma[idx];
    atomic_add(&sums[cell_index], (uint)value);
    atomic_add(&counts[cell_index], 1u);
}
