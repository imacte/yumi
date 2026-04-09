use std::process::Command;
use std::env;
use std::path::Path;

fn compile_bpf(src_name: &str, obj_name: &str, out_dir: &Path) -> std::path::PathBuf {
    let bpf_src = format!("src/bpf/{}", src_name);
    let bpf_header = "src/bpf/bpf_abi.h";
    let bpf_obj = out_dir.join(obj_name);

    // 监控源码变化，变化时自动重新编译
    println!("cargo:rerun-if-changed={}", bpf_src);
    println!("cargo:rerun-if-changed={}", bpf_header);

    let clang_path = "/usr/bin/clang";

    let status = Command::new(clang_path)
        .args([
            "-target", "bpfel-unknown-none", // 纯净 BPF 目标
            "-O2",
            "-c",
            "-fno-addrsig",
            "-fno-ident",
            &bpf_src,
            "-o",
            bpf_obj.to_str().unwrap(),
        ])
        .status()
        .unwrap_or_else(|_| panic!("无法执行 clang 编译 {}", src_name));

    if !status.success() {
        panic!("eBPF 编译失败: {}!", src_name);
    }

    // 这里会在执行 cargo build 时在控制台输出醒目的黄色日志！
    println!("cargo:warning=✅ 成功编译 eBPF 字节码: {} -> {}", src_name, obj_name);

    bpf_obj
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    // 1. 编译帧率探针
    let fps_obj = compile_bpf("fps_probe.c", "fps_probe.o", out_path);
    // 2. 编译 CPU 探针
    let cpu_obj = compile_bpf("cpu_probe.c", "cpu_probe.o", out_path);

    // 将两个产物的路径注入到环境变量中
    println!("cargo:rustc-env=BPF_FPS_OBJ_PATH={}", fps_obj.display());
    println!("cargo:rustc-env=BPF_CPU_OBJ_PATH={}", cpu_obj.display());
}