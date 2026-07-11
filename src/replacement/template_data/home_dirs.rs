//! amber_home — Replacement for `home` and `dirs` crates
//!
//! Provides home directory detection without dependencies.

use std::env;
use std::path::PathBuf;

/// Get the user's home directory
pub fn home_dir() -> Option<PathBuf> {
    // Unix: check HOME
    #[cfg(unix)]
    {
        env::var_os("HOME").map(PathBuf::from)
    }
    // Windows: check USERPROFILE, then HOMEDRIVE+HOMEPATH
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| {
                let drive = env::var_os("HOMEDRIVE")?;
                let path = env::var_os("HOMEPATH")?;
                let mut buf = PathBuf::from(drive);
                buf.push(path);
                Some(buf)
            })
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Get the user's config directory
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                let mut home = home_dir()?;
                home.push(".config");
                Some(home)
            })
    }
    #[cfg(windows)]
    {
        env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Get the user's cache directory
pub fn cache_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                let mut home = home_dir()?;
                home.push(".cache");
                Some(home)
            })
    }
    #[cfg(windows)]
    {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

