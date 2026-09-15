struct Dims {
    width: u32,
    height: u32,
    tiles_x: u32,
    tiles_y: u32,
};

@group(0) @binding(0) var<storage, read> luma: array<u32>;
@group(0) @binding(1) var<storage, read_write> histograms: array<atomic<u32>>;
@group(0) @binding(2) var<uniform> dims: Dims;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let x = gid.x;
    let y = gid.y;
    if (x >= dims.width || y >= dims.height) {
        return;
    }

    var tile_x = x * dims.tiles_x / dims.width;
    if (tile_x >= dims.tiles_x) {
        tile_x = dims.tiles_x - 1u;
    }
    var tile_y = y * dims.tiles_y / dims.height;
    if (tile_y >= dims.tiles_y) {
        tile_y = dims.tiles_y - 1u;
    }

    let region_index = tile_y * dims.tiles_x + tile_x;
    let value = luma[y * dims.width + x];
    atomicAdd(&histograms[region_index * 256u + value], 1u);
}
