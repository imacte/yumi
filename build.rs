use std::process::Command;
use std::env;
use std::path::PathBuf;

/// 构建 yumi-ebpf BPF 程序，参照 frame-analyzer 的 build_ebpf()
fn build_ebpf() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ebpf_dir = manifest_dir.join("yumi-ebpf");
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let target_dir = out_dir.join("ebpf_target");
    let tools_dir = out_dir.join("ebpf_tools");
    let tools_bin = tools_dir.join("bin");

    // 监控 ebpf crate 变化
    println!("cargo:rerun-if-changed={}", ebpf_dir.join("Cargo.toml").display());
    println!("cargo:rerun-if-changed={}", ebpf_dir.join("src").display());

    // 1. 安装 bpf-linker（参照 frame-analyzer install_ebpf_linker）
    Command::new("cargo")
        .args([
            "install", "bpf-linker", "--force",
            "--root", tools_dir.to_str().unwrap(),
            "--target-dir", tools_dir.to_str().unwrap(),
        ])
        .env_remove("RUSTUP_TOOLCHAIN")
        .status()?;

    // 2. 编译 BPF 程序（在 yumi-ebpf 目录中，避免 workspace 干扰）
    let mut ebpf_args = vec![
        "--target", "bpfel-unknown-none",
        "-Z", "build-std=core",
        "--target-dir", target_dir.to_str().unwrap(),
    ];

    #[cfg(not(debug_assertions))]
    ebpf_args.push("--release");

    let status = Command::new("cargo")
        .arg("build")
        .args(&ebpf_args)
        .current_dir(&ebpf_dir)
        .env_remove("RUSTUP_TOOLCHAIN")
        .env("PATH", add_path(&tools_bin)?)
        .status()?;

    if !status.success() {
        panic!("yumi-ebpf 编译失败");
    }

    // 调试：列出实际编译产物
    let find = Command::new("find")
        .arg(target_dir.to_str().unwrap())
        .arg("-name")
        .arg("yumi*")
        .arg("-type")
        .arg("f")
        .output();
    if let Ok(out) = find {
        println!("cargo:warning=yumi-ebpf find output: {}", String::from_utf8_lossy(&out.stdout).trim());
    }

    // 3. 产物路径（binary crate 直接输出到 <target>/<profile>/<name>，无 deps/hash）
    #[cfg(debug_assertions)]
    let profile = "debug";
    #[cfg(not(debug_assertions))]
    let profile = "release";

    let built_obj = target_dir
        .join("bpfel-unknown-none")
        .join(profile)
        .join("yumi-ebpf"); // binary crate 保留原始包名中的连字符

    // 复制到 OUT_DIR 根下（平铺路径，避免 include_bytes! 子目录访问问题）
    let flat_path = out_dir.join("bpf_probe.o");
    std::fs::copy(&built_obj, &flat_path)
        .unwrap_or_else(|e| panic!("无法复制到 {}: {}", flat_path.display(), e));

    Ok(flat_path)
}

fn add_path(add: &std::path::Path) -> Result<String, std::env::VarError> {
    let path = env::var("PATH")?;
    Ok(format!("{}:{}", add.display(), path))
}

fn main() {
    match build_ebpf() {
        Ok(bpf_obj) => {
            println!("cargo:warning=✅ yumi-ebpf 编译成功: {}", bpf_obj.display());
        }
        Err(e) => {
            panic!("yumi-ebpf 编译失败: {e}");
        }
    }
}
