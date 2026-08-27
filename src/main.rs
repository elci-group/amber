// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! Amber — Autonomous Dependency Reduction Engine
//!
//! Thin binary wrapper around the `amber` library CLI module.

fn main() {
    amber::cli::run_cli();
}
