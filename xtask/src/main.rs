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
    /// 更新版本号 (Cargo.toml + module.prop + update.json)
    #[command(alias = "r")]
    Release {
        /// 新版本号，如 "v1.0.7"
        version: String,
        /// 新 versionCode，不传则自动 +1
        #[arg(short, long)]
        code: Option<u32>,
    },
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
        Commands::Release { version, code } => release(&version, code)?,
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
    
    let webroot_dir = temp_dir.join("webroot");
    dir::copy(
        Path::new("webui").join("dist"),
        &webroot_dir,
        &dir::CopyOptions::new().overwrite(true).content_only(true),
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

// ─── Release 子命令 ─────────────────────────────────────

fn release(version: &str, code: Option<u32>) -> Result<()> {
    // 去掉前缀 v（如果用户传了）
    let ver_stripped = version.strip_prefix('v').unwrap_or(version);

    // 1. 更新 module.prop
    let prop_path = Path::new("module/module.prop");
    let prop = fs::read_to_string(prop_path)?;
    let new_code = match code {
        Some(c) => c,
        None => {
            // 从现有 module.prop 提取并 +1
            let current: u32 = prop
                .lines()
                .find(|l| l.starts_with("versionCode="))
                .and_then(|l| l.trim_start_matches("versionCode=").parse().ok())
                .unwrap_or(0);
            current + 1
        }
    };

    let prop = prop
        .lines()
        .map(|l| {
            if l.starts_with("version=") {
                format!("version=v{}", ver_stripped)
            } else if l.starts_with("versionCode=") {
                format!("versionCode={}", new_code)
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(prop_path, prop)?;

    // 2. 更新 Cargo.toml
    let cargo_path = Path::new("Cargo.toml");
    let cargo = fs::read_to_string(cargo_path)?;
    let cargo = cargo
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("version = ") {
                format!("version = \"{}\"", ver_stripped)
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(cargo_path, cargo)?;

    // 3. 更新 update.json
    let update_path = Path::new("updateInformation/update.json");
    if update_path.exists() {
        let json = fs::read_to_string(update_path)?;
        let json = json
            .lines()
            .map(|l| {
                if l.trim().starts_with("\"version\"") {
                    format!("  \"version\": \"v{}\",", ver_stripped)
                } else if l.trim().starts_with("\"versionCode\"") {
                    format!("  \"versionCode\": {},", new_code)
                } else if l.trim().starts_with("\"zipUrl\"") {
                    // 替换 URL 中的 yumi-vX.Y.Z
                    let start = l.find("yumi-v").unwrap_or(0);
                    let end = l[start..].find('/').unwrap_or(l.len() - start);
                    let rest = if start + end < l.len() { &l[start + end..] } else { "" };
                    // 注意：这里的文件名需要手动确认（CI 生成的 zip 名包含 git commit 和日期）
                    format!("  \"zipUrl\": \"https://github.com/imacte/yumi/releases/download/yumi-v{}/yumi-v{}.zip{},", ver_stripped, ver_stripped, rest.trim_end_matches(','))
                } else if l.trim().starts_with("\"changelog\"") {
                    format!("  \"changelog\": \"https://raw.githubusercontent.com/imacte/yumi/main/updateInformation/changelog.md\"")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(update_path, json)?;
    }

    println!("版本已更新为 v{} (versionCode={})", ver_stripped, new_code);
    println!("  ✓ Cargo.toml");
    println!("  ✓ module/module.prop");
    println!("  ✓ updateInformation/update.json (zipUrl 中的文件名请手动更新)");
    Ok(())
}
