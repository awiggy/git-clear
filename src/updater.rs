use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use semver::Version;
use serde::Deserialize;

/// Update source configuration. Defaults point at this project's own repository
/// (awiggy/git-clear); edition release builds (for example the Clear edition) override these
/// at compile time. build.rs declares `rerun-if-env-changed` for every variable so changing
/// them always rebuilds.
pub const REPOSITORY_URL: &str = match option_env!("GIT_AGENT_UPDATE_REPOSITORY_URL") {
    Some(value) => value,
    None => "https://github.com/awiggy/git-clear",
};
pub const RELEASE_PAGE_URL: &str = match option_env!("GIT_AGENT_UPDATE_RELEASE_PAGE_URL") {
    Some(value) => value,
    None => "https://github.com/awiggy/git-clear/releases",
};
/// Self-update stays off unless the build was produced with a dedicated release feed
/// (`GIT_AGENT_SELF_UPDATE=1`). An edition must never install another edition's package over
/// itself, so enabling this requires a distinct tag prefix and asset stem for the edition.
pub const SELF_UPDATE_ENABLED: bool = env_flag_enabled(option_env!("GIT_AGENT_SELF_UPDATE"));

const fn env_flag_enabled(value: Option<&str>) -> bool {
    match value {
        Some(flag) => flag.len() == 1 && flag.as_bytes()[0] == b'1',
        None => false,
    }
}
/// Tag prefix of this edition's release stream. Upstream uses `v*`; the Clear edition uses
/// `clear-v*`. The prefix lets both streams share one repository without mixing assets.
pub const RELEASE_TAG_PREFIX: &str = match option_env!("GIT_AGENT_UPDATE_TAG_PREFIX") {
    Some(value) => value,
    None => "v",
};
/// Installer asset name stem: `<stem>Setup-….exe`, `<stem>_…_amd64.deb`, `<stem>-…-macOS.dmg`.
pub const UPDATE_ASSET_STEM: &str = match option_env!("GIT_AGENT_UPDATE_ASSET_STEM") {
    Some(value) => value,
    None => "GitAgent",
};
const LATEST_RELEASE_API: &str = match option_env!("GIT_AGENT_UPDATE_LATEST_API") {
    Some(value) => value,
    None => "https://api.github.com/repos/awiggy/git-clear/releases/latest",
};
const RELEASES_API: &str = match option_env!("GIT_AGENT_UPDATE_RELEASES_API") {
    Some(value) => value,
    None => "https://api.github.com/repos/awiggy/git-clear/releases",
};
const TRUSTED_DOWNLOAD_PREFIX: &str = match option_env!("GIT_AGENT_UPDATE_DOWNLOAD_PREFIX") {
    Some(value) => value,
    None => "https://github.com/awiggy/git-clear/releases/download/",
};
const USER_AGENT: &str = "Git-Agent-Updater";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateRelease {
    pub tag: String,
    pub name: String,
    pub notes: String,
    pub page_url: String,
    pub is_newer: bool,
    pub asset: Option<ReleaseAsset>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallOutcome {
    InstallerLaunched,
    Installed,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: String,
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub fn check_latest_release(current_version: &str) -> Result<UpdateRelease> {
    let os = std::env::consts::OS;
    if RELEASE_TAG_PREFIX == "v" {
        let release: GithubRelease = fetch_json(LATEST_RELEASE_API)?;
        return release_from_response(current_version, release, os);
    }
    // Editions with their own tag prefix share the repository with upstream releases, so the
    // "latest release" endpoint would mix streams. List releases and pick the newest
    // published entry of this edition instead (the API returns newest first).
    let releases: Vec<GithubRelease> = fetch_json(RELEASES_API)?;
    let release = select_edition_release(releases, RELEASE_TAG_PREFIX).ok_or_else(|| {
        anyhow!("No published release found for the {RELEASE_TAG_PREFIX}* update stream")
    })?;
    release_from_response(current_version, release, os)
}

fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
    let response = ureq::get(url)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| anyhow!("GitHub release request failed: {error}"))?;
    response
        .into_json()
        .context("GitHub release response was not valid JSON")
}

fn select_edition_release(releases: Vec<GithubRelease>, tag_prefix: &str) -> Option<GithubRelease> {
    releases.into_iter().find(|release| {
        release.tag_name.starts_with(tag_prefix) && !release.draft && !release.prerelease
    })
}

pub fn download_and_install(asset: &ReleaseAsset) -> Result<InstallOutcome> {
    validate_release_asset(asset)?;
    let directory = update_temp_directory()?;
    fs::create_dir_all(&directory)
        .with_context(|| format!("Unable to create update directory: {}", directory.display()))?;
    let archive_path = directory.join(&asset.name);
    download_asset(asset, &archive_path)?;
    install_downloaded_asset(&archive_path, &directory)
}

fn release_from_response(
    current_version: &str,
    release: GithubRelease,
    os: &str,
) -> Result<UpdateRelease> {
    let current = parse_version(current_version)?;
    let latest = parse_version(&release.tag_name)?;
    Ok(UpdateRelease {
        tag: release.tag_name,
        name: release.name,
        notes: release.body.unwrap_or_default().trim().to_owned(),
        page_url: release.html_url,
        is_newer: latest > current,
        asset: platform_asset(&release.assets, os).map(|asset| ReleaseAsset {
            name: asset.name.clone(),
            download_url: asset.browser_download_url.clone(),
            size: asset.size,
        }),
    })
}

fn parse_version(raw: &str) -> Result<Version> {
    parse_version_with_prefix(raw, RELEASE_TAG_PREFIX)
}

fn parse_version_with_prefix(raw: &str, tag_prefix: &str) -> Result<Version> {
    let trimmed = raw.trim();
    let normalized = trimmed
        .strip_prefix(tag_prefix)
        .or_else(|| trimmed.strip_prefix('v'))
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    Version::parse(normalized).with_context(|| format!("Invalid version: {raw}"))
}

fn platform_asset<'a>(assets: &'a [GithubAsset], os: &str) -> Option<&'a GithubAsset> {
    if !matches!(os, "windows" | "linux" | "macos") {
        return None;
    }
    assets
        .iter()
        .find(|asset| asset_name_matches_os(&asset.name, os))
}

fn asset_name_matches_os(name: &str, os: &str) -> bool {
    asset_name_matches_os_stem(name, os, UPDATE_ASSET_STEM)
}

fn asset_name_matches_os_stem(name: &str, os: &str, stem: &str) -> bool {
    match os {
        "windows" => name.starts_with(&format!("{stem}Setup-")) && name.ends_with(".exe"),
        "linux" => name.starts_with(&format!("{stem}_")) && name.ends_with("_amd64.deb"),
        "macos" => name.starts_with(&format!("{stem}-")) && name.ends_with("-macOS.dmg"),
        _ => false,
    }
}

fn validate_release_asset(asset: &ReleaseAsset) -> Result<()> {
    if !asset.download_url.starts_with(TRUSTED_DOWNLOAD_PREFIX) {
        bail!("Update asset is not hosted by the official Git Agent release");
    }
    let file_name = Path::new(&asset.name)
        .file_name()
        .and_then(|name| name.to_str());
    if file_name != Some(asset.name.as_str()) || asset.name.is_empty() {
        bail!("Update asset has an invalid file name");
    }
    let valid_platform_name = asset_name_matches_os(&asset.name, std::env::consts::OS);
    if !valid_platform_name {
        bail!("Update asset does not match the current operating system");
    }
    Ok(())
}

fn update_temp_directory() -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("System clock is earlier than the Unix epoch")?
        .as_millis();
    Ok(std::env::temp_dir().join(format!(
        "git-agent-update-{}-{timestamp}",
        std::process::id()
    )))
}

fn download_asset(asset: &ReleaseAsset, destination: &Path) -> Result<()> {
    let response = ureq::get(&asset.download_url)
        .set("Accept", "application/octet-stream")
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| anyhow!("Update download failed: {error}"))?;
    let mut reader = response.into_reader();
    let mut file = File::create(destination)
        .with_context(|| format!("Unable to create update file: {}", destination.display()))?;
    let bytes = io::copy(&mut reader, &mut file).context("Unable to save update package")?;
    file.sync_all().context("Unable to finish update package")?;
    if bytes == 0 {
        bail!("Downloaded update package is empty");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_downloaded_asset(path: &Path, _directory: &Path) -> Result<InstallOutcome> {
    Command::new(path)
        .spawn()
        .with_context(|| format!("Unable to launch installer: {}", path.display()))?;
    Ok(InstallOutcome::InstallerLaunched)
}

#[cfg(target_os = "macos")]
fn install_downloaded_asset(path: &Path, _directory: &Path) -> Result<InstallOutcome> {
    Command::new("open")
        .arg(path)
        .spawn()
        .with_context(|| format!("Unable to open disk image: {}", path.display()))?;
    Ok(InstallOutcome::InstallerLaunched)
}

#[cfg(target_os = "linux")]
fn install_downloaded_asset(path: &Path, _directory: &Path) -> Result<InstallOutcome> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .or_else(|_| Command::new("gio").arg("open").arg(path).spawn())
        .with_context(|| {
            format!(
                "Unable to open system package installer: {}",
                path.display()
            )
        })?;
    Ok(InstallOutcome::InstallerLaunched)
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn install_downloaded_asset(_path: &Path, _directory: &Path) -> Result<InstallOutcome> {
    bail!("Automatic updates are not supported on this operating system")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release_json() -> GithubRelease {
        serde_json::from_str(
            r#"{
                "tag_name":"v1.2.0",
                "name":"Git Agent v1.2.0",
                "body":"Release notes",
                "html_url":"https://github.com/awiggy/git-clear/releases/tag/v1.2.0",
                "assets":[
                    {"name":"GitAgent_1.2.0_amd64.deb","browser_download_url":"https://github.com/awiggy/git-clear/releases/download/v1.2.0/GitAgent_1.2.0_amd64.deb","size":10},
                    {"name":"GitAgent-1.2.0-macOS.dmg","browser_download_url":"https://github.com/awiggy/git-clear/releases/download/v1.2.0/GitAgent-1.2.0-macOS.dmg","size":20},
                    {"name":"GitAgentSetup-v1.2.0.exe","browser_download_url":"https://github.com/awiggy/git-clear/releases/download/v1.2.0/GitAgentSetup-v1.2.0.exe","size":30}
                ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn semantic_versions_ignore_v_prefix_and_prerelease_ordering() {
        assert!(parse_version("v1.2.0").unwrap() > parse_version("1.1.9").unwrap());
        assert!(parse_version("1.2.0").unwrap() > parse_version("1.2.0-beta.1").unwrap());
        assert!(parse_version("not-a-version").is_err());
    }

    #[test]
    fn release_result_selects_platform_asset_and_compares_current_version() {
        let windows = release_from_response("1.1.0", release_json(), "windows").unwrap();
        assert!(windows.is_newer);
        assert_eq!(windows.asset.unwrap().name, "GitAgentSetup-v1.2.0.exe");

        let linux = release_from_response("1.2.0", release_json(), "linux").unwrap();
        assert!(!linux.is_newer);
        assert_eq!(linux.asset.unwrap().name, "GitAgent_1.2.0_amd64.deb");

        let macos = release_from_response("1.3.0", release_json(), "macos").unwrap();
        assert!(!macos.is_newer);
        assert_eq!(macos.asset.unwrap().name, "GitAgent-1.2.0-macOS.dmg");
        assert!(asset_name_matches_os("GitAgent-1.2.0-macOS.dmg", "macos"));
        assert!(asset_name_matches_os("GitAgent_1.2.0_amd64.deb", "linux"));
        assert!(!asset_name_matches_os(
            "git-agent-linux-installer.tar.gz",
            "linux"
        ));
    }

    #[test]
    fn update_asset_rejects_untrusted_downloads_and_path_names() {
        let untrusted = ReleaseAsset {
            name: "GitAgentSetup-v1.2.0.exe".to_owned(),
            download_url: "https://example.com/GitAgentSetup-v1.2.0.exe".to_owned(),
            size: 10,
        };
        assert!(validate_release_asset(&untrusted).is_err());

        let invalid_name = ReleaseAsset {
            name: "../GitAgentSetup-v1.2.0.exe".to_owned(),
            download_url: "https://github.com/awiggy/git-clear/releases/download/v1.2.0/GitAgentSetup-v1.2.0.exe".to_owned(),
            size: 10,
        };
        assert!(validate_release_asset(&invalid_name).is_err());
    }

    #[test]
    fn version_parsing_strips_the_edition_tag_prefix() {
        assert_eq!(
            parse_version_with_prefix("clear-v1.3.15", "clear-v").unwrap(),
            parse_version("1.3.15").unwrap()
        );
        // An upstream-style tag still parses when an edition prefix is active.
        assert_eq!(
            parse_version_with_prefix("v1.2.0", "clear-v").unwrap(),
            parse_version("1.2.0").unwrap()
        );
        assert!(parse_version_with_prefix("clear-vnot-a-version", "clear-v").is_err());
    }

    #[test]
    fn asset_matching_uses_the_edition_stem() {
        assert!(asset_name_matches_os_stem(
            "GitAgent-Clear-1.3.15-macOS.dmg",
            "macos",
            "GitAgent-Clear"
        ));
        assert!(asset_name_matches_os_stem(
            "GitAgent-ClearSetup-1.3.15.exe",
            "windows",
            "GitAgent-Clear"
        ));
        assert!(asset_name_matches_os_stem(
            "GitAgent-Clear_1.3.15_amd64.deb",
            "linux",
            "GitAgent-Clear"
        ));
        // Clear assets must not match the upstream stem and vice versa.
        assert!(!asset_name_matches_os_stem(
            "GitAgent-Clear-1.3.15-macOS.dmg",
            "macos",
            "GitAgentX"
        ));
        assert!(!asset_name_matches_os_stem(
            "GitAgent-1.2.0-macOS.dmg",
            "macos",
            "GitAgent-Clear"
        ));
    }

    #[test]
    fn edition_release_selection_skips_other_streams_drafts_and_prereleases() {
        let parse = |tag: &str, draft: bool, prerelease: bool| -> GithubRelease {
            serde_json::from_value(serde_json::json!({
                "tag_name": tag,
                "name": tag,
                "body": null,
                "html_url": format!("https://github.com/awiggy/git-clear/releases/tag/{tag}"),
                "draft": draft,
                "prerelease": prerelease,
                "assets": []
            }))
            .unwrap()
        };
        let releases = vec![
            parse("v9.9.9", false, false),
            parse("clear-v1.4.0", true, false),
            parse("clear-v1.3.15", false, true),
            parse("clear-v1.3.14", false, false),
        ];

        let selected = select_edition_release(releases, "clear-v").unwrap();
        assert_eq!(selected.tag_name, "clear-v1.3.14");
        assert!(select_edition_release(Vec::new(), "clear-v").is_none());
    }
}
