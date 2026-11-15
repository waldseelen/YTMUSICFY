use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use directories::ProjectDirs;

use crate::error::StoreError;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Cross-platform config, database, and snapshot directory layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl AppPaths {
    /// Resolve OS-standard directories (`io.ytm-architect.ytm-architect`) via `ProjectDirs`.
    pub fn from_project_dirs() -> Result<Self, StoreError> {
        let dirs = ProjectDirs::from("io", "ytm-architect", "ytm-architect")
            .ok_or(StoreError::ProjectDirUnavailable)?;
        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_local_dir().to_path_buf(),
        })
    }

    /// Construct isolated paths rooted under `root` (for tests and portable runs).
    #[must_use]
    pub fn from_root(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
        }
    }

    #[must_use]
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    #[must_use]
    pub fn auth_file(&self) -> PathBuf {
        self.config_dir.join("auth.json")
    }

    #[must_use]
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    #[must_use]
    pub fn genres_file(&self) -> PathBuf {
        self.config_dir.join("genres.toml")
    }

    #[must_use]
    pub fn db_file(&self) -> PathBuf {
        self.data_dir.join("ytm.sqlite")
    }

    #[must_use]
    pub fn snapshots_dir(&self) -> PathBuf {
        self.data_dir.join("snapshots")
    }

    /// Create `config_dir`, `data_dir`, and `snapshots_dir` if absent.
    pub fn ensure_dirs(&self) -> Result<(), StoreError> {
        for dir in [&self.config_dir, &self.data_dir, &self.snapshots_dir()] {
            fs::create_dir_all(dir).map_err(|source| StoreError::Io {
                path: dir.display().to_string(),
                source,
            })?;
        }
        Ok(())
    }
}

/// Atomically write `bytes` to `path` via a same-directory temp file + rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| StoreError::Io {
        path: parent.display().to_string(),
        source,
    })?;

    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp_path = parent.join(format!(".{file_name}.tmp.{}.{seq}", std::process::id()));

    {
        let mut f = fs::File::create(&tmp_path).map_err(|source| StoreError::Io {
            path: tmp_path.display().to_string(),
            source,
        })?;
        f.write_all(bytes).map_err(|source| StoreError::Io {
            path: tmp_path.display().to_string(),
            source,
        })?;
        f.sync_all().map_err(|source| StoreError::Io {
            path: tmp_path.display().to_string(),
            source,
        })?;
    }

    fs::rename(&tmp_path, path).map_err(|source| StoreError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(())
}

/// Write sensitive credentials atomically and restrict permissions per OS (`0600` on Unix).
pub fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    write_atomic(path, bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms).map_err(|source| StoreError::Io {
            path: path.display().to_string(),
            source,
        })?;
    }

    #[cfg(windows)]
    {
        tracing::warn!(
            path = %path.display(),
            "credentials saved on Windows; verify user-scoped NTFS ACLs restrict file access"
        );
    }

    Ok(())
}
