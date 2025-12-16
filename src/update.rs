// src/update.rs

use anyhow::Result;
use colored::Colorize;
use self_update::cargo_crate_version;

const REPO_OWNER: &str = "ops3000"; // <--- 替换为你的 GitHub 用户名
const REPO_NAME: &str = "ops-cli";  // <--- 替换为你的 GitHub 仓库名
const BIN_NAME: &str = "ops";       // 本地二进制文件名

pub fn check_for_update(verbose: bool) -> Result<Option<String>> {
    let current_version = cargo_crate_version!();
    
    // self_update 是同步阻塞的，我们在 async main 中使用 spawn_blocking 调用它
    // 避免阻塞 tokio 运行时
    let status = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .current_version(current_version)
        .build()?;

    let latest_release = status.get_latest_release()?;
    let latest_version = latest_release.version;

    let current = semver::Version::parse(current_version)?;
    let latest = semver::Version::parse(&latest_version)?;

    if latest > current {
        if verbose {
            println!("\n{}", "✨ New version available!".bold().yellow());
            println!("Current: {}", current_version.red());
            println!("Latest:  {}", latest_version.green());
            println!("Run `{}` to update.\n", "ops update".bold());
        }
        return Ok(Some(latest_version));
    }

    Ok(None)
}

pub fn update_self() -> Result<()> {
    let current_version = cargo_crate_version!();
    println!("Checking for updates...");

    let status = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .show_download_progress(true)
        .current_version(current_version)
        .no_confirm(true) // 自动确认
        .build()?;

    // 执行更新
    // 这会自动下载对应架构的 asset (如 ops-linux-amd64) 并替换当前可执行文件
    let update_status = status.update()?;

    if update_status.updated() {
        println!("{}", format!("✔ Successfully updated to version {}!", update_status.version()).green());
    } else {
        println!("{}", "You are already using the latest version.".green());
    }

    Ok(())
}