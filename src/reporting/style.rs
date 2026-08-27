// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! 3form-backed terminal styling compatibility surface.

pub use form3::compat::Colorize;

#[cfg(test)]
mod tests {
    use super::Colorize;
    use std::env;
    use std::io::IsTerminal;

    #[test]
    fn color_respects_terminal_policy() {
        let rendered = "test".red().to_string();
        if env::var_os("NO_COLOR").is_some()
            || (!std::io::stdout().is_terminal() && env::var_os("FORCE_COLOR").is_none())
        {
            assert_eq!(rendered, "test");
        } else {
            assert!(rendered.contains("\x1b[31m"));
            assert!(rendered.contains("\x1b[0m"));
        }
    }

    #[test]
    fn chaining_uses_a_single_terminal_reset() {
        let rendered = "test".red().bold().to_string();
        assert!(rendered == "test" || rendered.matches("\x1b[0m").count() == 1);
    }
}
