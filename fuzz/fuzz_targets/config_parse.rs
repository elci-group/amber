#![no_main]

use libfuzzer_sys::fuzz_target;

/// Fuzzes `.amber.toml` handling: TOML deserialization into `Config`,
/// validation, and policy lookups. The parser must never panic, loop, or
/// accept invalid weights/policy combinations on arbitrary input.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(config) = toml::from_str::<amber::config::Config>(text) else {
        return;
    };
    let _ = config.validate();
    if let Some(policy) = &config.policy {
        for name in ["serde", "anyhow", "comfy-table", ""] {
            let _ = policy.is_required(name);
            let _ = policy.is_forbidden(name);
        }
    }
});
