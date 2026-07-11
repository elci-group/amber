use crate::analysis::types::CrateUsage;

/// Result of attempting to generate a replacement for a crate.
#[derive(Debug, Clone)]
pub enum TemplateOutcome {
    /// A hand-written, crate-specific replacement module.
    Dedicated(String),
    /// A replacement generated from actual usage data (e.g. `itertools`).
    UsageDriven(String),
    /// No viable replacement is available for this crate.
    Unsupported { reason: String },
}

/// Code templates for common replacement scenarios
pub struct ReplacementTemplate<'a> {
    crate_name: &'a str,
    usage: &'a CrateUsage,
}

#[allow(clippy::unused_self)]
impl<'a> ReplacementTemplate<'a> {
    #[must_use]
    pub const fn for_crate(crate_name: &'a str, usage: &'a CrateUsage) -> Self {
        Self { crate_name, usage }
    }

    /// Generate replacement code for the configured crate.
    ///
    /// Returns [`TemplateOutcome::Unsupported`] for crates that do not yet have
    /// a dedicated or usage-driven replacement. No generic stubs are emitted.
    #[must_use]
    pub fn generate_code(&self) -> TemplateOutcome {
        match self.crate_name {
            "lazy_static" => TemplateOutcome::Dedicated(self.replace_lazy_static()),
            "once_cell" => TemplateOutcome::Dedicated(self.replace_once_cell()),
            "itertools" => TemplateOutcome::UsageDriven(self.replace_itertools()),
            "colored" | "owo-colors" | "yansi" | "ansi_term" => {
                TemplateOutcome::Dedicated(self.replace_colored())
            }
            "anyhow" => TemplateOutcome::Dedicated(self.replace_anyhow()),
            "thiserror" => TemplateOutcome::Dedicated(self.replace_thiserror()),
            "log" => TemplateOutcome::Dedicated(self.replace_log()),
            "env_logger" => TemplateOutcome::Dedicated(self.replace_env_logger()),
            "humantime" => TemplateOutcome::Dedicated(self.replace_humantime()),
            "home" | "dirs" => TemplateOutcome::Dedicated(self.replace_home_dirs()),
            "either" => TemplateOutcome::Dedicated(self.replace_either()),
            "tap" => TemplateOutcome::Dedicated(self.replace_tap()),
            "cfg-if" => TemplateOutcome::Dedicated(self.replace_cfg_if()),
            "maplit" => TemplateOutcome::Dedicated(self.replace_maplit()),
            "byteorder" => TemplateOutcome::Dedicated(self.replace_byteorder()),
            "chrono" => TemplateOutcome::Dedicated(self.replace_chrono()),
            "regex" => TemplateOutcome::Dedicated(self.replace_regex()),
            "tracing" => TemplateOutcome::Dedicated(self.replace_tracing()),
            "tracing-subscriber" => TemplateOutcome::Dedicated(self.replace_tracing_subscriber()),
            "toml" => TemplateOutcome::Dedicated(self.replace_toml()),
            "ureq" => TemplateOutcome::Dedicated(self.replace_ureq()),
            _ => TemplateOutcome::Unsupported {
                reason: format!(
                    "no dedicated replacement template for `{}`; add one to `src/replacement/template_data/`",
                    self.crate_name
                ),
            },
        }
    }

    fn replace_lazy_static(&self) -> String {
        include_str!("template_data/lazy_static.rs").to_string()
    }

    fn replace_once_cell(&self) -> String {
        include_str!("template_data/once_cell.rs").to_string()
    }

    fn replace_itertools(&self) -> String {
        let mut code = r"//! amber_itertools — Replacement for common `itertools` functions
//!
//! Only implements the subset of itertools actually used.

pub trait AmberItertools: Iterator + Sized {
"
        .to_string();

        // Only implement methods that are actually used
        for item in &self.usage.imported_items {
            match item.name.as_str() {
                "join" => {
                    code.push_str(
                        r#"
    /// Join iterator elements with a separator
    fn join(&mut self, sep: &str) -> String
    where
        Self::Item: std::fmt::Display,
    {
        use std::fmt::Write;
        let mut result = String::new();
        let mut first = true;
        for item in self {
            if !first {
                result.push_str(sep);
            }
            write!(result, "{}", item).unwrap();
            first = false;
        }
        result
    }
"#,
                    );
                }
                "collect_vec" => {
                    code.push_str(
                        r"
    /// Collect into a Vec
    fn collect_vec(self) -> Vec<Self::Item> {
        self.collect()
    }
",
                    );
                }
                "unique" => {
                    code.push_str(
                        r"
    /// Return iterator with consecutive duplicates removed
    fn unique(self) -> std::vec::IntoIter<Self::Item>
    where
        Self::Item: PartialEq + Clone,
    {
        let mut result = Vec::new();
        for item in self {
            if !result.contains(&item) {
                result.push(item);
            }
        }
        result.into_iter()
    }
",
                    );
                }
                "intersperse" => {
                    code.push_str(
                        r"
    /// Place separator between each element
    fn intersperse(self, sep: Self::Item) -> std::vec::IntoIter<Self::Item>
    where
        Self::Item: Clone,
    {
        let mut result = Vec::new();
        let mut first = true;
        for item in self {
            if !first {
                result.push(sep.clone());
            }
            result.push(item);
            first = false;
        }
        result.into_iter()
    }
",
                    );
                }
                _ => {}
            }
        }

        // Default implementations for common methods
        if !self
            .usage
            .imported_items
            .iter()
            .any(|i| ["join", "collect_vec", "unique", "intersperse"].contains(&i.name.as_str()))
        {
            code.push_str(
                r"
    /// Collect into a Vec (convenience method)
    fn collect_vec(self) -> Vec<Self::Item> {
        self.collect()
    }
",
            );
        }

        code.push_str("}\n\n");
        code.push_str(
            r"impl<T: Iterator + Sized> AmberItertools for T {}

/// Re-export for drop-in replacement
pub use AmberItertools as Itertools;
",
        );

        code
    }

    fn replace_colored(&self) -> String {
        include_str!("template_data/colored.rs").to_string()
    }

    fn replace_anyhow(&self) -> String {
        include_str!("template_data/anyhow.rs").to_string()
    }

    fn replace_thiserror(&self) -> String {
        include_str!("template_data/thiserror.rs").to_string()
    }

    fn replace_log(&self) -> String {
        include_str!("template_data/log.rs").to_string()
    }

    fn replace_env_logger(&self) -> String {
        include_str!("template_data/env_logger.rs").to_string()
    }

    fn replace_humantime(&self) -> String {
        include_str!("template_data/humantime.rs").to_string()
    }

    fn replace_home_dirs(&self) -> String {
        include_str!("template_data/home_dirs.rs").to_string()
    }

    fn replace_either(&self) -> String {
        include_str!("template_data/either.rs").to_string()
    }

    fn replace_tap(&self) -> String {
        include_str!("template_data/tap.rs").to_string()
    }

    fn replace_cfg_if(&self) -> String {
        include_str!("template_data/cfg_if.rs").to_string()
    }

    fn replace_maplit(&self) -> String {
        include_str!("template_data/maplit.rs").to_string()
    }

    fn replace_byteorder(&self) -> String {
        include_str!("template_data/byteorder.rs").to_string()
    }

    fn replace_chrono(&self) -> String {
        include_str!("template_data/chrono.rs").to_string()
    }

    fn replace_regex(&self) -> String {
        include_str!("template_data/regex.rs").to_string()
    }

    fn replace_tracing(&self) -> String {
        include_str!("template_data/tracing.rs").to_string()
    }

    fn replace_tracing_subscriber(&self) -> String {
        include_str!("template_data/tracing_subscriber.rs").to_string()
    }

    fn replace_toml(&self) -> String {
        include_str!("template_data/toml.rs").to_string()
    }

    fn replace_ureq(&self) -> String {
        include_str!("template_data/ureq.rs").to_string()
    }
}

#[cfg(test)]
impl TemplateOutcome {
    fn code(&self) -> &str {
        match self {
            Self::Dedicated(code) | Self::UsageDriven(code) => code,
            Self::Unsupported { reason } => reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::types::{CrateUsage, ImportedItem, ItemKind, Location};
    use crate::replacement::validator::Validator;

    fn usage_for(crate_name: &str) -> CrateUsage {
        CrateUsage {
            crate_name: crate_name.to_string(),
            ..Default::default()
        }
    }

    fn usage_with_items(crate_name: &str, items: &[&str]) -> CrateUsage {
        CrateUsage {
            crate_name: crate_name.to_string(),
            imported_items: items
                .iter()
                .map(|name| ImportedItem {
                    name: (*name).to_string(),
                    kind: ItemKind::Function,
                    path: format!("{crate_name}::{name}"),
                    location: Location::new("src/lib.rs", 1, 1),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn lazy_static_template_contains_oncelock() {
        let usage = usage_for("lazy_static");
        let template = ReplacementTemplate::for_crate("lazy_static", &usage);
        let code = template.generate_code();
        assert!(matches!(code, TemplateOutcome::Dedicated(_)));
        assert!(code.code().contains("OnceLock"));
    }

    #[test]
    fn byteorder_template_has_helpers() {
        let usage = usage_for("byteorder");
        let template = ReplacementTemplate::for_crate("byteorder", &usage);
        let code = template.generate_code();
        assert!(matches!(code, TemplateOutcome::Dedicated(_)));
        assert!(code.code().contains("read_u32_le"));
        assert!(code.code().contains("write_u32_be"));
    }

    #[test]
    fn chrono_template_has_timestamp() {
        let usage = usage_for("chrono");
        let template = ReplacementTemplate::for_crate("chrono", &usage);
        let code = template.generate_code();
        assert!(matches!(code, TemplateOutcome::Dedicated(_)));
        assert!(code.code().contains("SystemTime"));
    }

    #[test]
    fn tracing_template_has_macros() {
        let usage = usage_for("tracing");
        let template = ReplacementTemplate::for_crate("tracing", &usage);
        let code = template.generate_code();
        assert!(matches!(code, TemplateOutcome::Dedicated(_)));
        assert!(code.code().contains("macro_rules! info"));
        assert!(code.code().contains("pub struct Span"));
    }

    #[test]
    fn tracing_subscriber_template_has_init() {
        let usage = usage_for("tracing-subscriber");
        let template = ReplacementTemplate::for_crate("tracing-subscriber", &usage);
        let code = template.generate_code();
        assert!(matches!(code, TemplateOutcome::Dedicated(_)));
        assert!(code.code().contains("pub fn init()"));
        assert!(code.code().contains("pub struct FmtSubscriber"));
    }

    #[test]
    fn toml_template_has_value() {
        let usage = usage_for("toml");
        let template = ReplacementTemplate::for_crate("toml", &usage);
        let code = template.generate_code();
        assert!(matches!(code, TemplateOutcome::Dedicated(_)));
        assert!(code.code().contains("pub enum Value"));
        assert!(code.code().contains("pub fn parse"));
    }

    #[test]
    fn ureq_template_has_request() {
        let usage = usage_for("ureq");
        let template = ReplacementTemplate::for_crate("ureq", &usage);
        let code = template.generate_code();
        assert!(matches!(code, TemplateOutcome::Dedicated(_)));
        assert!(code.code().contains("pub struct Request"));
        assert!(code.code().contains("pub fn get"));
    }

    #[test]
    fn new_templates_pass_cargo_check() {
        let validator = Validator::new();
        for crate_name in ["tracing", "tracing-subscriber", "toml", "ureq"] {
            let usage = usage_for(crate_name);
            let template = ReplacementTemplate::for_crate(crate_name, &usage);
            let outcome = template.generate_code();
            let code = outcome.code();
            let module_name = format!("amber_{}_redux", crate_name.replace('-', "_"));
            let report = validator.validate(&module_name, code).unwrap();
            assert!(
                report.success,
                "template for {crate_name} failed validation: {report:?}"
            );
        }
    }

    #[test]
    fn unknown_crate_returns_unsupported() {
        let usage = usage_for("unknown_crate");
        let template = ReplacementTemplate::for_crate("unknown_crate", &usage);
        let code = template.generate_code();
        assert!(
            matches!(code, TemplateOutcome::Unsupported { .. }),
            "expected unsupported outcome, got {code:?}"
        );
    }

    #[test]
    fn all_known_templates_generate_non_empty_code() {
        let known = [
            "lazy_static",
            "once_cell",
            "itertools",
            "colored",
            "owo-colors",
            "yansi",
            "ansi_term",
            "anyhow",
            "thiserror",
            "log",
            "env_logger",
            "humantime",
            "home",
            "dirs",
            "either",
            "tap",
            "cfg-if",
            "maplit",
            "byteorder",
            "chrono",
            "regex",
            "tracing",
            "tracing-subscriber",
            "toml",
            "ureq",
        ];
        for crate_name in known {
            let usage = usage_for(crate_name);
            let template = ReplacementTemplate::for_crate(crate_name, &usage);
            let code = template.generate_code();
            assert!(
                !code.code().is_empty(),
                "template for {crate_name} is empty"
            );
        }
    }

    #[test]
    fn itertools_template_includes_used_methods() {
        let usage = usage_with_items(
            "itertools",
            &["join", "collect_vec", "unique", "intersperse"],
        );
        let template = ReplacementTemplate::for_crate("itertools", &usage);
        let outcome = template.generate_code();
        let code = outcome.code();
        assert!(code.contains("fn join"));
        assert!(code.contains("fn collect_vec"));
        assert!(code.contains("fn unique"));
        assert!(code.contains("fn intersperse"));
    }

    #[test]
    fn unknown_crate_does_not_emit_stub_items() {
        let usage = usage_with_items("unknown", &["Foo", "Bar"]);
        let template = ReplacementTemplate::for_crate("unknown_crate", &usage);
        let code = template.generate_code();
        assert!(
            matches!(code, TemplateOutcome::Unsupported { .. }),
            "expected unsupported outcome"
        );
        assert!(!code.code().contains("TODO"));
    }

    #[test]
    fn itertools_unknown_item_adds_default_collect_vec() {
        let usage = usage_with_items("itertools", &["unknown_method"]);
        let template = ReplacementTemplate::for_crate("itertools", &usage);
        let outcome = template.generate_code();
        let code = outcome.code();
        assert!(code.contains("fn collect_vec"));
    }

    #[test]
    fn itertools_no_known_items_adds_default_collect_vec() {
        let usage = usage_for("itertools");
        let template = ReplacementTemplate::for_crate("itertools", &usage);
        let outcome = template.generate_code();
        let code = outcome.code();
        assert!(code.contains("fn collect_vec"));
    }

    #[test]
    fn improved_templates_behave_functionally() {
        use std::process::Command;

        let temp = crate::temp::tempdir().unwrap();
        let dir = temp.path();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"tpl_check\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        let base =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/replacement/template_data");
        std::fs::copy(base.join("toml.rs"), dir.join("src/amber_toml.rs")).unwrap();
        std::fs::copy(
            base.join("tracing_subscriber.rs"),
            dir.join("src/amber_tracing_subscriber.rs"),
        )
        .unwrap();
        std::fs::copy(base.join("ureq.rs"), dir.join("src/amber_ureq.rs")).unwrap();

        std::fs::write(
            dir.join("src/main.rs"),
            r##"
mod amber_toml;
mod amber_tracing_subscriber;
mod amber_ureq;

fn main() {
    let doc = r#"
threshold = 60
name = "example"
ratio = 1.5
enabled = true
tags = ["a", "b", "c"]

[policy]
strict = false
forbidden = ["serde", "deprecated"]

[library]
enabled = true
path = "~/.amber/lib.pad"
"#;
    let v = amber_toml::parse(doc).expect("parse");
    assert_eq!(v.get_path("threshold").and_then(|x| x.as_integer()), Some(60));
    assert_eq!(v.get_path("name").and_then(|x| x.as_str()), Some("example"));
    assert!(matches!(v.get_path("ratio"), Some(amber_toml::Value::Float(_))));
    assert_eq!(v.get_path("enabled").and_then(|x| x.as_bool()), Some(true));
    match v.get_path("tags").unwrap() {
        amber_toml::Value::Array(a) => assert_eq!(a.len(), 3),
        _ => panic!("tags should be an array"),
    }
    assert_eq!(v.get_path("policy.strict").and_then(|x| x.as_bool()), Some(false));
    match v.get_path("policy.forbidden").unwrap() {
        amber_toml::Value::Array(a) => assert_eq!(a.len(), 2),
        _ => panic!("forbidden should be an array"),
    }
    assert_eq!(v.get_path("library.path").and_then(|x| x.as_str()), Some("~/.amber/lib.pad"));

    let serialized = amber_toml::to_string(&v);
    let v2 = amber_toml::parse(&serialized).expect("reparse");
    assert_eq!(v2.get_path("threshold").and_then(|x| x.as_integer()), Some(60));

    use amber_tracing_subscriber::{enabled, set_max_level, EnvFilter, Level};
    set_max_level(Level::Warn);
    assert!(enabled(Level::Error));
    assert!(enabled(Level::Warn));
    assert!(!enabled(Level::Info));
    assert_eq!(EnvFilter::new("debug").max_level(), Level::Debug);
    assert_eq!(EnvFilter::new("amber=trace,info").max_level(), Level::Trace);

    let https = amber_ureq::get("https://example.com");
    assert!(https.is_err());
    assert!(https.unwrap_err().to_string().contains("HTTPS"));
    let refused = amber_ureq::get("http://127.0.0.1:9/");
    assert!(refused.is_err());

    println!("functional template assertions passed");
}
"##,
        )
        .unwrap();

        let status = Command::new("cargo")
            .arg("run")
            .arg("--quiet")
            .current_dir(dir)
            .status()
            .expect("run functional template assertions");
        assert!(status.success(), "functional template assertions failed");
    }
}
