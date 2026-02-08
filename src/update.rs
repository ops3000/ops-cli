use anyhow::{Context, Result};
use colored::Colorize;
use self_update::cargo_crate_version;
use self_update::backends::github::ReleaseList;
use self_update::update::Release;

const REPO_OWNER: &str = "ops3000";
const REPO_NAME: &str = "ops-cli";
const BIN_NAME: &str = "ops";

fn get_asset_name() -> &'static str {
    if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "ops-linux-amd64.tar.gz"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "ops-darwin-amd64.tar.gz"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "ops-darwin-arm64.tar.gz"
    } else {
        panic!("Unsupported platform for self-update")
    }
}

fn fetch_latest_release() -> Result<Release> {
    let releases = ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()?
        .fetch()?;

    releases
        .into_iter()
        .next()
        .context("No releases found on GitHub")
}

pub fn check_for_update(verbose: bool) -> Result<Option<String>> {
    let current_version = cargo_crate_version!();
    let release = fetch_latest_release()?;

    let current = semver::Version::parse(current_version)?;
    let latest = semver::Version::parse(&release.version)?;

    if latest > current {
        if verbose {
            o_warn!("\n{}", "✨ New version available!".bold().yellow());
            o_detail!("Current: {}", current_version.red());
            o_detail!("Latest:  {}", release.version.green());
            o_detail!("Run `{}` to update.\n", "ops update".bold());
        }
        return Ok(Some(release.version));
    }

    Ok(None)
}

pub fn update_self() -> Result<()> {
    let current_version = cargo_crate_version!();
    o_step!("Checking for updates...");

    let release = fetch_latest_release()?;
    let current = semver::Version::parse(current_version)?;
    let latest = semver::Version::parse(&release.version)?;

    if latest <= current {
        o_success!("{}", "You are already using the latest version.".green());
        return Ok(());
    }

    o_step!(
        "Updating {} → {}",
        current_version.red(),
        release.version.green()
    );

    let asset_name = get_asset_name();
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .with_context(|| format!("Asset '{}' not found in release {}", asset_name, release.version))?;

    let tmp_dir = tempfile::Builder::new()
        .prefix("ops-update")
        .tempdir()
        .context("Failed to create temp directory")?;

    let tmp_tarball_path = tmp_dir.path().join(asset_name);
    let tmp_tarball = std::fs::File::create(&tmp_tarball_path)?;

    // Use browser_download_url pattern instead of the API URL
    // (API URL requires Accept: application/octet-stream header which causes http crate conflicts)
    let download_url = format!(
        "https://github.com/{}/{}/releases/download/v{}/{}",
        REPO_OWNER, REPO_NAME, release.version, asset_name
    );

    self_update::Download::from_url(&download_url)
        .show_progress(true)
        .download_to(&tmp_tarball)?;

    let bin_name = std::path::PathBuf::from(BIN_NAME);
    self_update::Extract::from_source(&tmp_tarball_path)
        .archive(self_update::ArchiveKind::Tar(Some(self_update::Compression::Gz)))
        .extract_file(tmp_dir.path(), &bin_name)?;

    let new_exe = tmp_dir.path().join(BIN_NAME);
    let current_exe = std::env::current_exe()?;

    self_update::Move::from_source(&new_exe)
        .replace_using_temp(&current_exe)
        .to_dest(&current_exe)?;

    o_success!(
        "{}",
        format!("✔ Successfully updated to version {}!", release.version).green()
    );

    Ok(())
}

/// Check for updates and auto-update if available.
/// Returns Ok(true) if updated (caller should exit and re-run).
/// Silently returns Ok(false) on network errors.
pub fn check_and_auto_update() -> Result<bool> {
    let current_version = cargo_crate_version!();

    // Silently skip if network fails
    let release = match fetch_latest_release() {
        Ok(r) => r,
        Err(_) => return Ok(false),
    };

    let current = match semver::Version::parse(current_version) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let latest = match semver::Version::parse(&release.version) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };

    if latest > current {
        o_step!(
            "{}",
            format!("🔄 Updating ops {} → {}...", current, latest).yellow()
        );

        // Perform the update
        let asset_name = get_asset_name();
        let asset = match release.assets.iter().find(|a| a.name == asset_name) {
            Some(a) => a,
            None => return Ok(false),
        };

        let tmp_dir = tempfile::Builder::new()
            .prefix("ops-update")
            .tempdir()?;

        let tmp_tarball_path = tmp_dir.path().join(asset_name);
        let _tmp_tarball = std::fs::File::create(&tmp_tarball_path)?;

        let download_url = format!(
            "https://github.com/{}/{}/releases/download/v{}/{}",
            REPO_OWNER, REPO_NAME, release.version, asset_name
        );

        self_update::Download::from_url(&download_url)
            .show_progress(false)
            .download_to(&std::fs::File::create(&tmp_tarball_path)?)?;

        let bin_name = std::path::PathBuf::from(BIN_NAME);
        self_update::Extract::from_source(&tmp_tarball_path)
            .archive(self_update::ArchiveKind::Tar(Some(self_update::Compression::Gz)))
            .extract_file(tmp_dir.path(), &bin_name)?;

        let new_exe = tmp_dir.path().join(BIN_NAME);
        let current_exe = std::env::current_exe()?;

        self_update::Move::from_source(&new_exe)
            .replace_using_temp(&current_exe)
            .to_dest(&current_exe)?;

        o_success!(
            "{}",
            format!("✔ Updated to {}. Please re-run your command.", release.version).green()
        );

        return Ok(true);
    }

    Ok(false)
}
