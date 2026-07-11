/// Dependencies that should NEVER be replaced (security critical)
pub const NEVER_REPLACE: &[&str] = &[
    // Cryptography
    "openssl",
    "rustls",
    "ring",
    "aes",
    "aes-gcm",
    "chacha20poly1305",
    "rsa",
    "ecdsa",
    "ed25519",
    "x25519-dalek",
    "sha2",
    "sha3",
    "blake2",
    "blake3",
    "argon2",
    "bcrypt",
    "scrypt",
    "pbkdf2",
    "hmac",
    "hkdf",
    // TLS/Security protocols
    "tokio-rustls",
    "tokio-openssl",
    "native-tls",
    "schannel",
    "security-framework",
    // Randomness
    "rand",
    "rand_core",
    "getrandom",
    "uuid", // Often used for security-sensitive IDs
    // Serialization safety
    "serde",
    // Parsing safety
    "nom",
    "pest",
    "lalrpop",
    // Async runtime (too complex)
    "tokio",
    "async-std",
    "smol",
];

/// Dependencies that are FREQUENTLY replaceable (simple utilities)
pub const FREQUENTLY_REPLACEABLE: &[&str] = &[
    // String utilities
    "lazy_static",
    "once_cell",
    "itertools",
    "either",
    "tap",
    "maplit",
    // Color/formatting
    "colored",
    "ansi_term",
    "termcolor",
    "owo-colors",
    "yansi",
    // Small utilities
    "cfg-if",
    "autocfg",
    "version_check",
    // Logging (can often be replaced)
    "log",
    "env_logger",
    "pretty_env_logger",
    "simplelog",
    // Time/date helpers
    "chrono",
    "time",
    "humantime",
    // Path/utils
    "home",
    "dirs",
    "which",
    // Error handling (sometimes replaceable)
    "anyhow",
    "thiserror",
    "snafu",
    "eyre",
    // Collections
    "indexmap",
    "hashbrown",
    "smallvec",
    "arrayvec",
    "tinyvec",
];

/// Crates that often pull in heavy transitive dependencies
pub const HEAVY_TRANSITIVE_CRATES: &[&str] = &[
    "reqwest",
    "clap",
    "tokio",
    "hyper",
    "serde_json",
    "syn",
    "chrono",
    "regex",
    "uuid",
    "rand",
    "futures",
];

/// Calculate risk penalty based on crate category
#[must_use]
pub fn category_risk_penalty(crate_name: &str) -> i16 {
    if NEVER_REPLACE.contains(&crate_name) {
        return -50; // Major penalty
    }
    if FREQUENTLY_REPLACEABLE.contains(&crate_name) {
        return 15; // Bonus
    }
    0
}

/// Calculate bonus/penalty based on transitive dependency weight
#[must_use]
pub const fn transitive_weight_score(transitive_count: usize) -> i16 {
    match transitive_count {
        0 => 5,
        1..=3 => 3,
        4..=10 => 0,
        11..=30 => -5,
        31..=100 => -10,
        _ => -15,
    }
}

/// Score based on usage complexity
#[must_use]
pub const fn usage_complexity_score(unique_apis: usize, call_sites: usize) -> i16 {
    let api_score = match unique_apis {
        0 => -20,      // Unused or gated - good candidate
        1 => 20,       // Single API - very replaceable
        2..=3 => 15,   // Few APIs
        4..=10 => 5,   // Moderate
        11..=30 => -5, // Many APIs - harder
        _ => -15,      // Extensive usage - very hard
    };

    let call_score = match call_sites {
        0..=5 => 10,
        6..=20 => 5,
        21..=100 => 0,
        _ => -5,
    };

    api_score + call_score
}

/// Score based on maintenance burden
#[must_use]
pub fn maintenance_score(cve_count: usize, maintenance_percent: u8, _update_frequency: f64) -> i16 {
    // Cap CVE count to avoid overflow on unusual targets and keep the penalty bounded.
    let capped_cve_count = if cve_count > 100 { 100 } else { cve_count };
    let cve_penalty = -(i16::try_from(capped_cve_count).unwrap_or(100) * 10);

    let maint_score = match maintenance_percent {
        0..=20 => -15, // Poorly maintained
        21..=50 => -5,
        51..=70 => 0,
        71..=85 => 5,
        _ => 10, // Well maintained
    };

    cve_penalty + maint_score
}

/// Get human-readable category for a crate
#[must_use]
pub fn categorize_crate(crate_name: &str) -> &'static str {
    let name = crate_name.to_lowercase();

    if name.contains("serde")
        || name.contains("json")
        || name.contains("yaml")
        || name.contains("toml")
    {
        return "serialization";
    }
    if name.contains("crypto")
        || name.contains("aes")
        || name.contains("sha")
        || name.contains("hash")
    {
        return "cryptography";
    }
    if name.contains("tls")
        || name.contains("ssl")
        || name.contains("rustls")
        || name.contains("openssl")
    {
        return "tls";
    }
    if name.contains("http")
        || name.contains("hyper")
        || name.contains("reqwest")
        || name.contains("axum")
    {
        return "networking";
    }
    if name.contains("async") || name.contains("tokio") || name.contains("future") {
        return "async-runtime";
    }
    if name.contains("regex") || name.contains("parse") || name.contains("nom") {
        return "parsing";
    }
    if name.contains("log") || name.contains("tracing") {
        return "logging";
    }
    if name.contains("color") || name.contains("term") || name.contains("ansi") {
        return "terminal";
    }
    if name.contains("time") || name.contains("chrono") || name.contains("date") {
        return "datetime";
    }
    if name.contains("rand") || name.contains("uuid") {
        return "randomness";
    }
    if name.contains("test") || name.contains("mock") || name.contains("fuzz") {
        return "testing";
    }
    if name.contains("clap") || name.contains("structopt") || name.contains("argh") {
        return "cli";
    }

    "general"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_replace_contains_crypto() {
        assert!(NEVER_REPLACE.contains(&"serde"));
        assert!(NEVER_REPLACE.contains(&"tokio"));
        assert!(!NEVER_REPLACE.contains(&"lazy_static"));
    }

    #[test]
    fn frequently_replaceable_contains_utilities() {
        assert!(FREQUENTLY_REPLACEABLE.contains(&"itertools"));
        assert!(FREQUENTLY_REPLACEABLE.contains(&"anyhow"));
        assert!(!FREQUENTLY_REPLACEABLE.contains(&"openssl"));
    }

    #[test]
    fn category_risk_penalty_reflects_lists() {
        assert_eq!(category_risk_penalty("serde"), -50);
        assert_eq!(category_risk_penalty("lazy_static"), 15);
        assert_eq!(category_risk_penalty("unknown_crate"), 0);
    }

    #[test]
    fn transitive_weight_score_scales_with_count() {
        assert_eq!(transitive_weight_score(0), 5);
        assert_eq!(transitive_weight_score(2), 3);
        assert_eq!(transitive_weight_score(7), 0);
        assert_eq!(transitive_weight_score(101), -15);
    }

    #[test]
    fn usage_complexity_score_balances_apis_and_calls() {
        assert_eq!(usage_complexity_score(0, 0), -10); // -20 + 10
        assert_eq!(usage_complexity_score(1, 3), 30); // 20 + 10
        assert_eq!(usage_complexity_score(5, 50), 5); // 5 + 0
    }

    #[test]
    fn maintenance_score_reflects_cves_and_maint() {
        assert_eq!(maintenance_score(0, 90, 4.0), 10); // 0 + 10
        assert_eq!(maintenance_score(2, 50, 4.0), -25); // -20 + -5
    }

    #[test]
    fn categorize_crate_classifies_by_name() {
        assert_eq!(categorize_crate("serde_json"), "serialization");
        assert_eq!(categorize_crate("tokio"), "async-runtime");
        assert_eq!(categorize_crate("colored"), "terminal");
        assert_eq!(categorize_crate("foobar"), "general");
    }

    #[test]
    fn categorize_crate_covers_all_branches() {
        assert_eq!(categorize_crate("serde_yaml"), "serialization");
        assert_eq!(categorize_crate("crypto_hash"), "cryptography");
        assert_eq!(categorize_crate("rustls"), "tls");
        assert_eq!(categorize_crate("hyper"), "networking");
        assert_eq!(categorize_crate("regex"), "parsing");
        assert_eq!(categorize_crate("tracing"), "logging");
        assert_eq!(categorize_crate("chrono"), "datetime");
        assert_eq!(categorize_crate("rand"), "randomness");
        assert_eq!(categorize_crate("mockall"), "testing");
        assert_eq!(categorize_crate("clap"), "cli");
    }

    #[test]
    fn transitive_weight_score_covers_mid_ranges() {
        assert_eq!(transitive_weight_score(15), -5);
        assert_eq!(transitive_weight_score(50), -10);
    }

    #[test]
    fn usage_complexity_score_covers_high_usage() {
        assert_eq!(usage_complexity_score(20, 150), -10); // -5 + -5
        assert_eq!(usage_complexity_score(50, 0), -5); // -15 + 10
    }

    #[test]
    fn maintenance_score_caps_high_cve_count() {
        assert_eq!(maintenance_score(150, 90, 1.0), -990); // -(100*10) + 10
    }
}
