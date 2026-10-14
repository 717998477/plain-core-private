use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const BUNDLE: &str = "src/web_dist";

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let build_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=PLAINAPP_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=PLAINAPP_BUILD_TIME={build_time}");
    println!("cargo:rerun-if-changed=build.rs");
    embed_web_bundle();
}

fn embed_web_bundle() {
    println!("cargo:rerun-if-changed={BUNDLE}");
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join(BUNDLE);
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    if files.is_empty() {
        println!(
            "cargo:warning={BUNDLE} is missing, so plainapp is built without the web UI. Build the frontend and rebuild, or pass `run --web-root <directory>`."
        );
    }
    files.sort();
    let entries = files
        .iter()
        .map(|(key, path)| {
            format!(
                "    ({:?}, include_bytes!({:?})),",
                key,
                path.to_str().unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let generated = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("web_bundle.rs");
    fs::write(
        generated,
        format!("pub static FILES: &[(&str, &[u8])] = &[\n{entries}\n];\n"),
    )
    .unwrap();
}

fn collect(root: &Path, directory: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap().to_str().unwrap().to_owned();
        if relative.split('/').any(|segment| segment.starts_with('.')) {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            files.push((relative, path));
        }
    }
}
