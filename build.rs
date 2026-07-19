use std::process::Command;
use std::env;
use std::path::Path;

/// 使用 cargo 编译 Rust eBPF 程序，目标为 bpfel-unknown-none
fn compile_rust_bpf(manifest_dir: &Path, package_name: &str, out_dir: &Path) -> std::path::PathBuf {
    let obj_name = format!("{}.o", package_name);
    let bpf_obj = out_dir.join(&obj_name);

    // 监控整个 ebpf crate 的代码变化
    let ebpf_dir = manifest_dir.join(package_name);
    println!("cargo:rerun-if-changed={}", ebpf_dir.join("Cargo.toml").display());
    println!("cargo:rerun-if-changed={}", ebpf_dir.join("src").display());

    let output = Command::new("cargo")
        .args([
            "build",
            "--package", package_name,
            "--target", "bpfel-unknown-none",
            "--release",
            "-Z", "build-std=core",
        ])
        .current_dir(manifest_dir)
        .output()
        .unwrap_or_else(|e| panic!("无法执行 cargo build 编译 {}: {}", package_name, e));

    // 将子 cargo 的 stdout/stderr 转发到父进程，确保 CI 日志可见
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.is_empty() {
        eprintln!("[yumi-ebpf stdout]\n{stdout}");
    }
    if !stderr.is_empty() {
        eprintln!("[yumi-ebpf stderr]\n{stderr}");
    }

    if !output.status.success() {
        panic!("Rust eBPF 编译失败: {}!\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}", package_name);
    }

    // aya 编译产物在 target/bpfel-unknown-none/release/<package_name>
    let built_obj = manifest_dir
        .join("target")
        .join("bpfel-unknown-none")
        .join("release")
        .join(package_name);

    // 复制到 OUT_DIR 供 include_bytes_aligned! 使用
    std::fs::copy(&built_obj, &bpf_obj)
        .unwrap_or_else(|e| panic!("无法复制 eBPF 产物: {} -> {}: {}", built_obj.display(), bpf_obj.display(), e));

    println!("cargo:warning=✅ 成功编译 Rust eBPF 程序: {} -> {}", package_name, obj_name);

    bpf_obj
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    // 统一编译 yumi-ebpf（包含 fps_probe + cpu_probe 两个 BPF 程序）
    let bpf_obj = compile_rust_bpf(manifest_dir, "yumi-ebpf", out_path);

    // 统一的环境变量，fps_monitor 和 cpu_monitor 共用
    println!("cargo:rustc-env=BPF_OBJ_PATH={}", bpf_obj.display());
}
