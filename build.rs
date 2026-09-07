fn main() {
    println!("cargo:rerun-if-changed=assets/icons/git-agent.ico");

    // Compile-time update-source overrides consumed by src/updater.rs via option_env!.
    for variable in [
        "GIT_AGENT_SELF_UPDATE",
        "GIT_AGENT_UPDATE_REPOSITORY_URL",
        "GIT_AGENT_UPDATE_RELEASE_PAGE_URL",
        "GIT_AGENT_UPDATE_LATEST_API",
        "GIT_AGENT_UPDATE_RELEASES_API",
        "GIT_AGENT_UPDATE_DOWNLOAD_PREFIX",
        "GIT_AGENT_UPDATE_TAG_PREFIX",
        "GIT_AGENT_UPDATE_ASSET_STEM",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }

    #[cfg(target_os = "windows")]
    winresource::WindowsResource::new()
        .set_icon("assets/icons/git-agent.ico")
        .compile()
        .expect("failed to embed the Git Agent Windows icon");
}
