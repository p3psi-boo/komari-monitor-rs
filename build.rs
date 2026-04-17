use std::process::Command;

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(trimmed.to_string())
}

fn build_version() -> String {
    let cargo_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
    let profile = std::env::var("PROFILE").unwrap_or_default();
    let is_release_like = matches!(profile.as_str(), "release" | "minimal");

    if !is_release_like {
        return cargo_version;
    }

    let commit_date = git_output(&["log", "-1", "--date=format:%Y-%m-%d", "--format=%cd"]);
    let commit_hash = git_output(&["rev-parse", "--short", "HEAD"]);

    match (commit_date, commit_hash) {
        (Some(date), Some(hash)) => format!("{date}-{hash}"),
        _ => cargo_version,
    }
}

fn main() {
    let version = build_version();
    println!("cargo:rustc-env=KOMARI_BUILD_VERSION={version}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-env-changed=PROFILE");

    #[cfg(all(feature = "winxp-support", target_os = "windows"))]
    thunk::thunk();
}
