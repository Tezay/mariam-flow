//! Stamps the build with the commit it was made from.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (output.status.success() && !text.is_empty()).then(|| text.to_owned())
}

fn main() {
    // The reflog moves with every commit, pull and checkout, which neither
    // `HEAD` nor a single ref does; without it a build made after a pull that
    // left this crate untouched would keep the previous commit.
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/logs/HEAD");
    }

    // Semantic-versioning build metadata. A source tree with no repository
    // around it, a release archive for instance, carries the version alone.
    let version = env!("CARGO_PKG_VERSION");
    match git(&["rev-parse", "--short=10", "HEAD"]) {
        Some(commit) => println!("cargo:rustc-env=FLOW_EDGE_VERSION={version}+{commit}"),
        None => println!("cargo:rustc-env=FLOW_EDGE_VERSION={version}"),
    }
}
