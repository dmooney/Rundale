use std::process::Command;

fn main() {
    // Rerun when the workspace's git HEAD changes so branch/timestamp stay fresh.
    // The crate now lives at crates/parish-tauri/, so .git is two levels up.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");

    // Embed git branch name at compile time
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=PARISH_GIT_BRANCH={}", branch);

    // Embed build timestamp
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    println!("cargo:rustc-env=PARISH_BUILD_TIME={}", now);

    tauri_build::build()
}
