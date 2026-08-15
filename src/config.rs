//! Where the app remembers which database to use, between runs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::mariadb::MariaDbSettings;
use crate::db::mssql::MsSqlSettings;

/// Which backend the user chose on the Database tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    #[default]
    Sqlite,
    MariaDb,
    MsSql,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub backend: Backend,
    /// `None` means the default file in the app's data directory.
    pub sqlite_path: Option<PathBuf>,
    pub mariadb: MariaDbSettings,
    pub mssql: MsSqlSettings,
    /// Whether the MariaDB/SQL Server password may be written to the config
    /// file. Off by default: the file is plain text, and saying so is better
    /// than a surprise.
    pub remember_password: bool,
    /// Whether to ask GitHub at startup if there is a newer release.
    ///
    /// On by default, off in one click, and worth being explicit about: it is
    /// the only thing this app does over the network unasked, and it tells
    /// GitHub someone is running it.
    #[serde(default = "yes")]
    pub check_for_updates: bool,
    /// A version the user has already been told about and did not want.
    pub dismissed_update: Option<String>,
}

/// `serde`'s default for a `bool` is `false`, and this one is `true`.
fn yes() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backend: Backend::default(),
            sqlite_path: None,
            mariadb: MariaDbSettings::default(),
            mssql: MsSqlSettings::default(),
            remember_password: false,
            check_for_updates: true,
            dismissed_update: None,
        }
    }
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
            to_write.mssql.password.clear();
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
        self.sqlite_path
            .as_deref()
            .map_or_else(default_sqlite_path, untilde)
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

/// The other direction: turn a leading `~` back into the home directory.
///
/// A text box has no shell behind it to do this, so without it `~/accounts.db`
/// means a directory actually named `~`, created wherever the app happens to
/// have been started from — which for a windowed app is `/` on macOS and the
/// home directory on Linux. One fails, the other quietly succeeds and hands
/// back an empty database.
///
/// It matters because [`tilde`] puts that exact form on screen: the status bar
/// offers `~/Library/…`, and the obvious thing to do with a path you can see
/// is to type it back in.
pub fn untilde(path: &Path) -> PathBuf {
    // `~user` is somebody else's home directory, which this cannot resolve
    // and should not guess at; `strip_prefix` matches whole components, so it
    // declines that and leaves the path alone.
    match path.strip_prefix("~") {
        Ok(rest) => match dirs::home_dir() {
            Some(home) => home.join(rest),
            None => path.to_path_buf(),
        },
        Err(_) => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the status bar shows has to be something the Database tab will
    /// take back, or the app is telling the user a path it cannot then open.
    #[test]
    fn a_displayed_path_can_be_typed_back_in() {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        let path = home.join("Somewhere").join("accounts.sqlite");

        let shown = tilde(&path);
        assert!(shown.starts_with("~/"), "{shown}");
        assert_eq!(untilde(Path::new(&shown)), path);
    }

    #[test]
    fn other_paths_are_left_exactly_as_they_are() {
        for untouched in ["/var/db/accounts.sqlite", "accounts.sqlite", "~user/db"] {
            assert_eq!(untilde(Path::new(untouched)), PathBuf::from(untouched));
        }
    }
}
