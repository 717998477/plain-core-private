use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct DataPaths {
    root: PathBuf,
}
impl DataPaths {
    pub fn new(directory: &Path) -> Self {
        Self {
            root: directory.to_path_buf(),
        }
    }
    pub fn lock(&self) -> PathBuf {
        self.root.join(".core.lock")
    }
    pub fn pending_restore(&self) -> PathBuf {
        self.root.join(".restore-pending.json")
    }
    pub fn database(&self) -> PathBuf {
        self.root.join("plain.db")
    }
    pub fn system_prefs(&self) -> PathBuf {
        self.root.join("system_prefs.json")
    }
    pub fn user_prefs(&self) -> PathBuf {
        self.root.join("user_prefs.json")
    }
    pub fn kv(&self) -> PathBuf {
        self.root.join("fjall")
    }
    pub fn note_images(&self) -> PathBuf {
        self.root.join("note-images")
    }
    pub fn audio(&self) -> PathBuf {
        self.root.join("audio")
    }
    pub fn pomodoro_sound(&self) -> PathBuf {
        self.audio().join("pomodoro_sound.mp3")
    }
    pub fn crash_report(&self) -> PathBuf {
        self.root.join("crash_report.txt")
    }
    pub fn crash_log(&self) -> PathBuf {
        self.root.join("crash_log.txt")
    }
    pub fn tls_identity(&self) -> PathBuf {
        self.root.join("tls-identity.json")
    }
    pub fn initialize_directories(&self) -> std::io::Result<()> {
        for directory in [self.note_images(), self.audio()] {
            if std::fs::symlink_metadata(&directory).is_ok_and(|metadata| metadata.is_symlink()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Application directory cannot be a symbolic link",
                ));
            }
            std::fs::create_dir_all(directory)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/unit/data_paths.rs"]
mod tests;
