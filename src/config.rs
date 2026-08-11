//! Where the app remembers which database to use, between runs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::mariadb::MariaDbSettings;

/// Which backend the user chose on the Database tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    #[default]
    Sqlite,
    MariaDb,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub backend: Backend,
    /// `None` means the default file in the app's data directory.
    pub sqlite_path: Option<PathBuf>,
    pub mariadb: MariaDbSettings,
    /// Whether the MariaDB password may be written to the config file. Off by
    /// default: the file is plain text, and saying so is better than a
    /// surprise.
    pub remember_password: bool,
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str(&text) {
            Ok(config) => config,
            Err(err) => {
                log::warn!("ignoring unreadable config at {}: {err}", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut to_write = self.clone();
        if !to_write.remember_password {
            to_write.mariadb.password.clear();
        }
        let json = serde_json::to_string_pretty(&to_write)
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        // The file can hold a database password, so keep it to its owner —
        // from the moment it is created, not a moment afterwards, since a
        // file that is briefly world-readable is a file that was readable.
        #[cfg(unix)]
        {
            use std::io::Write as _;
            use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)?;
            file.write_all(json.as_bytes())?;
            // `mode` above only applies when creating, so tighten an existing
            // file that was written by an earlier, laxer version.
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        #[cfg(not(unix))]
        std::fs::write(&path, json)?;

        Ok(())
    }

    /// The SQLite file to use: whatever the user set, or the default.
    pub fn sqlite_path(&self) -> PathBuf {
        self.sqlite_path.clone().unwrap_or_else(default_sqlite_path)
    }
}

/// `~/Library/Application Support/GenericAccountingSystem` on macOS, the
/// equivalent elsewhere, and the current directory if the platform will not
/// say — an app that cannot find a home directory should still start.
fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("GenericAccountingSystem")
}

pub fn default_sqlite_path() -> PathBuf {
    data_dir().join("accounts.sqlite")
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

/// Shorten a path for display, so a status bar shows `~/Library/…` rather
/// than the user's name.
pub fn tilde(path: &Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}
