struct Dims {
    width: u32,
    height: u32,
    columns: u32,
    rows: u32,
};

@group(0) @binding(0) var<storage, read> luma: array<u32>;
@group(0) @binding(1) var<storage, read_write> sums: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> counts: array<atomic<u32>>;
@group(0) @binding(3) var<uniform> dims: Dims;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let x = gid.x;
    let y = gid.y;
    if (x >= dims.width || y >= dims.height) {
        return;
    }

    var col = x * dims.columns / dims.width;
    if (col >= dims.columns) {
        col = dims.columns - 1u;
    }
    var row = y * dims.rows / dims.height;
    if (row >= dims.rows) {
        row = dims.rows - 1u;
    }

    let cell_index = row * dims.columns + col;
    let value = luma[y * dims.width + x];
    atomicAdd(&sums[cell_index], value);
    atomicAdd(&counts[cell_index], 1u);
}
