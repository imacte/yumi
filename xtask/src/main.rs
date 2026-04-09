mod zip_ext;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::{Parser, Subcommand};
use fs_extra::{dir, file};
use serde::Deserialize;
use xshell::{cmd, Shell};
use zip::{write::FileOptions, CompressionMethod};

use crate::zip_ext::zip_create_from_directory_with_options;

#[derive(Parser)]
#[command(name = "xtask", about = "Yumi Build System")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 编译 Yumi 项目并打包
    #[command(alias = "b")]
    Build,
}

#[derive(Deserialize)]
struct Package {
    version: String,
}

#[derive(Deserialize)]
struct CargoConfig {
    package: Package,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // 初始化 xshell
    let sh = Shell::new()?;

    match cli.command {
        Commands::Build => build(&sh)?,
    }

    Ok(())
}

fn cal_git_code(sh: &Shell) -> Result<usize> {
    // xshell 极大地简化了获取命令 stdout 的过程
    let output = cmd!(sh, "git rev-list --count HEAD").read()?;
    Ok(output.trim().parse::<usize>()?)
}

fn get_date() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M").to_string()
}

fn build(sh: &Shell) -> Result<()> {
    let temp_dir = temp_dir();
    
    // 读取 Cargo.toml (注意：因为通过 `cargo xtask` 运行，工作目录是项目根目录)
    let toml_content = fs::read_to_string("Cargo.toml")?;
    let data: CargoConfig = toml::from_str(&toml_content)?;

    // 1. 清理并重建临时目录
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir)?;

    // 2. 编译 WebUI
    build_webui(sh)?;

    // 3. 编译 Rust 核心
    build_core(sh)?;

    // 4. 拷贝 module 目录内容
    let module_dir = Path::new("module").to_path_buf();
    dir::copy(
        &module_dir,
        &temp_dir,
        &dir::CopyOptions::new().overwrite(true).content_only(true),
    )?;

    if temp_dir.join(".gitignore").exists() {
        fs::remove_file(temp_dir.join(".gitignore"))?;
    }

    // 5. 组装 bin 目录
    let bin_path = temp_dir.join("core").join("bin");
    fs::create_dir_all(&bin_path)?;
    
    file::copy(
        aarch64_bin_path(),
        bin_path.join("yumi"),
        &file::CopyOptions::new().overwrite(true),
    )?;

    // 6. 打包 Zip
    let output_dir = Path::new("output");
    fs::create_dir_all(output_dir)?; // 确保 output 目录存在
    
    let zip_filename = format!(
        "yumi-{}-{}-{}.zip",
        data.package.version,
        cal_git_code(sh)?,
        get_date()
    );
    let zip_path = output_dir.join(zip_filename);

    println!("开始打包: {}", zip_path.display());

    let options: FileOptions<'_, ()> = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(9));
        
    zip_create_from_directory_with_options(&zip_path, &temp_dir, |_| options)?;

    println!("构建并打包成功！");
    Ok(())
}

fn temp_dir() -> PathBuf {
    Path::new("output").join(".temp")
}

fn aarch64_bin_path() -> PathBuf {
    Path::new("target")
        .join("aarch64-linux-android")
        .join("release")
        .join("yumi")
}

fn build_core(sh: &Shell) -> Result<()> {
    println!("正在编译 Rust Core...");
    // push_env 会在当前作用域内设置环境变量，离开作用域自动恢复
    let _env = sh.push_env("RUSTFLAGS", "-C default-linker-libraries");
    cmd!(sh, "cargo +nightly ndk --platform 26 -t arm64-v8a build -Z build-std -r").run()?;
    Ok(())
}

fn build_webui(sh: &Shell) -> Result<()> {
    println!("正在编译 WebUI...");
    // push_dir 类似于 cd，离开作用域后会自动切回原目录
    let _dir = sh.push_dir("webui");
    cmd!(sh, "npm run build").run()?;
    Ok(())
}