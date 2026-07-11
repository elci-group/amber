//! Internal ANSI styling utilities.
//!
//! Std-only replacement for the `colored` crate. Respects `NO_COLOR`.

use std::env;

const RESET: &str = "\x1b[0m";

/// Returns true if the `NO_COLOR` environment variable is set.
fn no_color() -> bool {
    env::var_os("NO_COLOR").is_some()
}

/// Wraps `s` in the ANSI SGR sequence `code` when coloring is enabled.
fn style(s: &str, code: &str) -> String {
    if no_color() {
        s.to_owned()
    } else {
        format!("\x1b[{code}m{s}{RESET}")
    }
}

/// Trait for applying ANSI styles to strings.
pub trait Colorize {
    /// Color the text red.
    fn red(&self) -> String;
    /// Color the text green.
    fn green(&self) -> String;
    /// Color the text yellow.
    fn yellow(&self) -> String;
    /// Color the text blue.
    fn blue(&self) -> String;
    /// Color the text magenta.
    fn magenta(&self) -> String;
    /// Color the text cyan.
    fn cyan(&self) -> String;
    /// Color the text bright red.
    fn bright_red(&self) -> String;
    /// Color the text bright green.
    fn bright_green(&self) -> String;
    /// Color the text bright yellow.
    fn bright_yellow(&self) -> String;
    /// Make the text bold.
    fn bold(&self) -> String;
    /// Make the text dimmed.
    fn dimmed(&self) -> String;
    /// Make the text italic.
    fn italic(&self) -> String;
    /// Underline the text.
    fn underline(&self) -> String;
}

impl Colorize for &str {
    fn red(&self) -> String {
        style(self, "31")
    }

    fn green(&self) -> String {
        style(self, "32")
    }

    fn yellow(&self) -> String {
        style(self, "33")
    }

    fn blue(&self) -> String {
        style(self, "34")
    }

    fn magenta(&self) -> String {
        style(self, "35")
    }

    fn cyan(&self) -> String {
        style(self, "36")
    }

    fn bright_red(&self) -> String {
        style(self, "91")
    }

    fn bright_green(&self) -> String {
        style(self, "92")
    }

    fn bright_yellow(&self) -> String {
        style(self, "93")
    }

    fn bold(&self) -> String {
        style(self, "1")
    }

    fn dimmed(&self) -> String {
        style(self, "2")
    }

    fn italic(&self) -> String {
        style(self, "3")
    }

    fn underline(&self) -> String {
        style(self, "4")
    }
}

impl Colorize for String {
    fn red(&self) -> String {
        style(self, "31")
    }

    fn green(&self) -> String {
        style(self, "32")
    }

    fn yellow(&self) -> String {
        style(self, "33")
    }

    fn blue(&self) -> String {
        style(self, "34")
    }

    fn magenta(&self) -> String {
        style(self, "35")
    }

    fn cyan(&self) -> String {
        style(self, "36")
    }

    fn bright_red(&self) -> String {
        style(self, "91")
    }

    fn bright_green(&self) -> String {
        style(self, "92")
    }

    fn bright_yellow(&self) -> String {
        style(self, "93")
    }

    fn bold(&self) -> String {
        style(self, "1")
    }

    fn dimmed(&self) -> String {
        style(self, "2")
    }

    fn italic(&self) -> String {
        style(self, "3")
    }

    fn underline(&self) -> String {
        style(self, "4")
    }
}

#[cfg(test)]
mod tests {
    use super::Colorize;
    use std::env;

    #[test]
    fn color_respects_no_color() {
        let s = "test".red();
        if env::var_os("NO_COLOR").is_some() {
            assert_eq!(s, "test");
        } else {
            assert!(s.contains("\x1b[31m"));
            assert!(s.contains("\x1b[0m"));
        }
    }

    #[test]
    fn chaining_respects_no_color() {
        let s = "test".red().bold();
        if env::var_os("NO_COLOR").is_some() {
            assert_eq!(s, "test");
        } else {
            assert!(s.contains("\x1b[31m"));
            assert!(s.contains("\x1b[1m"));
        }
    }
}
