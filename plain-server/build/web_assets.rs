use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn generate(root: &Path, output: &Path, required: bool) -> io::Result<()> {
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == io::ErrorKind::NotFound && !required => {
            println!(
                "cargo:warning=Web bundle missing; set PLAIN_WEB_DIST to a built frontend directory to embed the Web UI"
            );
        }
        metadata => {
            if !metadata?.file_type().is_dir() {
                return Err(io::Error::other(
                    "Web bundle must be a directory, not a symbolic link",
                ));
            }
            collect(root, root, &mut files)?;
            if !files.iter().any(|(name, _)| name == "index.html") {
                return Err(io::Error::other("Web bundle requires index.html"));
            }
        }
    }
    files.sort();
    let entries = files
        .iter()
        .map(|(name, path)| {
            format!(
                "    ({name:?}, include_bytes!({:?})),",
                path.to_str().unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        output,
        format!("static FILES: &[(&str, &[u8])] = &[\n{entries}\n];\n"),
    )
}

fn collect(root: &Path, directory: &Path, files: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| io::Error::other("Web asset name must be UTF-8"))?;
        if name.starts_with('.') {
            continue;
        }
        let kind = entry.file_type()?;
        println!("cargo:rerun-if-changed={}", path.display());
        if kind.is_dir() {
            collect(root, &path, files)?;
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|part| part.as_os_str().to_str().unwrap())
                .collect::<Vec<_>>()
                .join("/");
            files.push((relative, path));
        } else {
            return Err(io::Error::other(
                "Web assets must be regular files or directories",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/build/web_assets.rs"]
mod tests;
