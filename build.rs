use std::process::Command;

fn main() {
    let is_git = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let git_desc = if is_git {
        let commit = Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let dirty = Command::new("git")
            .args(["diff", "--quiet"])
            .status()
            .map(|s| !s.success())
            .unwrap_or(false);

        if dirty {
            format!("{}-dirty", commit)
        } else {
            commit
        }
    } else {
        "no-git".to_string()
    };

    let build_date = Command::new("date")
        .args(["+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_DESC={}", git_desc);
    println!("cargo:rustc-env=BUILD_DATE={}", build_date);
    println!("cargo:rerun-if-changed=.git/HEAD");
}
