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
    let nvcc = env::var("NVCC").unwrap_or_else(|_| "nvcc".to_string());
    println!("cargo:rerun-if-env-changed=NVCC");

    let target = env::var("TARGET").unwrap_or_default();
    let mut cc_build = cc::Build::new();
    cc_build.target(&target);
    cc_build.host(&env::var("HOST").unwrap_or_default());
    let compiler = cc_build.get_compiler();
    let cl_path = compiler.path();
    let cl_dir = cl_path.parent().unwrap_or_else(|| Path::new(""));

    let kernels = ["normalize_cells", "scan_pixels"];

    for kernel in kernels {
        let cu_path = format!("src/lib/compute/kernels/{kernel}.cu");
        let ptx_path = Path::new(&out_dir).join(format!("{kernel}.ptx"));

        println!("cargo:rerun-if-changed={cu_path}");

        let output = Command::new(&nvcc)
            .arg("--ptx")
            .arg("-allow-unsupported-compiler")
            .arg("-ccbin")
            .arg(cl_dir)
            .arg(&cu_path)
            .arg("-o")
            .arg(&ptx_path)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                println!(
                    "cargo:warning=CUDA: {kernel}.cu скомпилирован в PTX при сборке ({nvcc}) — NVRTC в рантайме не понадобится"
                );
            }
            Ok(output) => {
                println!(
                    "cargo:warning=CUDA: {nvcc} завершился с ошибкой на {kernel}.cu ({}) — откат на NVRTC в рантайме для этого ядра",
                    output.status
                );
                for line in String::from_utf8_lossy(&output.stderr).lines() {
                    println!("cargo:warning=  nvcc[{kernel}]: {line}");
                }
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    println!("cargo:warning=  nvcc[{kernel}]: {line}");
                }
                let _ = std::fs::write(&ptx_path, "");
            }
            Err(error) => {
                println!(
                    "cargo:warning=CUDA: не удалось запустить {nvcc} для {kernel}.cu ({error}) — откат на NVRTC в рантайме для этого ядра"
                );
                let _ = std::fs::write(&ptx_path, "");
            }
        }
    }
}
