// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! Self-update: clone the latest source and install it with Baby.

use crate::amber_anyhow::Result;
use crate::anyhow;
use std::env;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Clone the latest `amber` source and install it via `baby --user`.
///
/// # Errors
///
/// Returns an error if the clone or install step fails.
pub fn run() -> Result<i32> {
    let repo_url = env!("CARGO_PKG_REPOSITORY");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let checkout_dir = env::temp_dir().join(format!("amber-update-{}-{nonce}", std::process::id()));

    if checkout_dir.exists() {
        std::fs::remove_dir_all(&checkout_dir)
            .map_err(|e| anyhow!("failed to clean up {}: {e}", checkout_dir.display()))?;
    }

    println!("Cloning latest amber from {repo_url}…");
    let clone_status = Command::new("git")
        .args(["clone", "--depth", "1", repo_url])
        .arg(&checkout_dir)
        .status()
        .map_err(|e| anyhow!("failed to run git clone: {e}"))?;
    if !clone_status.success() {
        return Err(anyhow!("git clone of {repo_url} failed"));
    }

    println!("Installing with 'baby --user'…");
    let install_result = Command::new("baby")
        .arg("--user")
        .current_dir(&checkout_dir)
        .status();

    let _ = std::fs::remove_dir_all(&checkout_dir);

    let install_status = install_result
        .map_err(|e| anyhow!("failed to run 'baby --user' (is baby installed and on PATH?): {e}"))?;
    if !install_status.success() {
        return Err(anyhow!("'baby --user' install failed"));
    }

    println!("amber updated to the latest version");
    Ok(0)
}
