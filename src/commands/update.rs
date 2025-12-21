use anyhow::Result;
use colored::Colorize;
use self_update::cargo_crate_version;

const REPO_OWNER: &str = "ops3000";
const REPO_NAME: &str = "ops-cli";
const BIN_NAME: &str = "ops"; 

pub fn check_for_update(verbose: bool) -> Result<Option<String>> {
    let current_version = cargo_crate_version!();
    
    // 使用 cargo-only 的配置，尽量减少对 system ssl 的依赖
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

    // 配置更新器
    // 注意：因为我们在 release.yml 中使用了 .tar.gz 打包，
    // self_update 会自动下载、解压并替换当前运行的二进制文件。
    let status = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .show_download_progress(true)
        .current_version(current_version)
        .no_confirm(true)
        .build()?;

    let update_status = status.update()?;

    if update_status.updated() {
        println!("{}", format!("✔ Successfully updated to version {}!", update_status.version()).green());
    } else {
        println!("{}", "You are already using the latest version.".green());
    }

    Ok(())
}