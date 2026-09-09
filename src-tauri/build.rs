use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    tauri_build::build();
    compile_cuda_kernels();
}

fn compile_cuda_kernels() {
    if env::var("CARGO_FEATURE_CUDA").is_err() {
        return;
    }

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    let ptx_path = Path::new(&out_dir).join("normalize_cells.ptx");
    let cu_path = "src/lib/compute/kernels/normalize_cells.cu";

    println!("cargo:rerun-if-changed={cu_path}");
    println!("cargo:rerun-if-env-changed=NVCC");

    let nvcc = env::var("NVCC").unwrap_or_else(|_| "nvcc".to_string());
    let output = Command::new(&nvcc)
        .arg("--ptx")
        .arg(cu_path)
        .arg("-o")
        .arg(&ptx_path)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!(
                "cargo:warning=CUDA: ядро скомпилировано в PTX при сборке ({nvcc}) — NVRTC в рантайме не понадобится"
            );
        }
        Ok(output) => {
            println!(
                "cargo:warning=CUDA: {nvcc} завершился с ошибкой ({}) — откат на компиляцию PTX в рантайме через NVRTC",
                output.status
            );
            for line in String::from_utf8_lossy(&output.stderr).lines() {
                println!("cargo:warning=  nvcc: {line}");
            }
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                println!("cargo:warning=  nvcc: {line}");
            }
            let _ = std::fs::write(&ptx_path, "");
        }
        Err(error) => {
            println!(
                "cargo:warning=CUDA: не удалось запустить {nvcc} ({error}) — откат на компиляцию PTX в рантайме через NVRTC"
            );
            let _ = std::fs::write(&ptx_path, "");
        }
    }
}
