//! Amber — Autonomous Dependency Reduction Engine
//!
//! Library crate for analyzing Rust projects and scoring dependencies
//! for replaceability. The `amber` binary is a thin CLI wrapper around
//! this library.

#![warn(clippy::pedantic, clippy::nursery)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod amber_anyhow;
pub mod analysis;
pub mod cli;
pub mod config;
pub mod metadata;
pub mod replacement;
pub mod reporting;
pub mod scoring;
pub mod temp;

#[cfg(feature = "library")]
pub mod library;
