use std::{env, error::Error, fs, path::PathBuf, process::Command};

fn main() {
    if let Err(error) = build_ebpf() {
        panic!("eBPF build failed: {error}");
    }
}

fn build_ebpf() -> Result<(), Box<dyn Error>> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let tools = out.join("ebpf_tools");
    println!("cargo:rerun-if-changed=yumi-ebpf/Cargo.toml");
    println!("cargo:rerun-if-changed=yumi-ebpf/src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=Cargo.lock");

    // Respect an installed linker; install only when none is available.
    let linker = tools
        .join("bin")
        .join(format!("bpf-linker{}", env::consts::EXE_SUFFIX));
    if Command::new("bpf-linker")
        .arg("--version")
        .output()
        .is_err()
        && !linker.exists()
    {
        let status = Command::new("cargo")
            .args(["install", "bpf-linker", "--locked", "--root"])
            .arg(&tools)
            .env_remove("RUSTUP_TOOLCHAIN")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .env_remove("RUSTFLAGS")
            .status()?;
        if !status.success() {
            return Err("failed to install bpf-linker".into());
        }
    }
    let mut paths = vec![tools.join("bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let target = out.join("ebpf_target");
    // Always optimize probes: the BPF verifier must not see debug stack spills.
    let status = Command::new("cargo")
        .args([
            "build",
            "--package",
            "yumi-ebpf",
            "--bins",
            "--release",
            "--target",
            "bpfel-unknown-none",
            "-Z",
            "build-std=core",
            "--target-dir",
        ])
        .arg(&target)
        .current_dir(&manifest)
        .env("PATH", env::join_paths(paths)?)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .status()?;
    if !status.success() {
        return Err("failed to compile eBPF probes".into());
    }
    for name in ["cpu", "fps-ring", "fps-perf"] {
        fs::copy(
            target.join("bpfel-unknown-none/release").join(name),
            out.join(format!("{name}.ebpf")),
        )?;
    }
    Ok(())
}
