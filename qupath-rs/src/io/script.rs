//! QuPath (Groovy) script lines for the Workflow tab, built so that names taken
//! from files or typed by users cannot break out of their string literals.

use serde_json::Value;

/// A Groovy single-quoted string literal (`'…'`). Backslashes, quotes and control
/// characters are escaped; single-quoted Groovy strings never interpolate `$`.
pub fn groovy_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// `runPlugin('class', '{json}')` with the parameters JSON-encoded, then quoted.
pub fn run_plugin(class: &str, params: &Value) -> String {
    format!("runPlugin({}, {})", groovy_str(class), groovy_str(&params.to_string()))
}

/// `name(arg)` with one string argument.
pub fn call(name: &str, arg: &str) -> String {
    format!("{name}({})", groovy_str(arg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_cannot_end_the_literal() {
        let evil = "x'); \"rm -rf ~\".execute(); ('";
        let s = groovy_str(evil);
        assert_eq!(s, "'x\\'); \"rm -rf ~\".execute(); (\\''");
        // Every quote inside is escaped, so only the outer two remain unescaped.
        let unescaped = s.match_indices('\'').filter(|(i, _)| *i == 0 || !s[..*i].ends_with('\\')).count();
        assert_eq!(unescaped, 2);
    }

    #[test]
    fn newlines_and_backslashes_are_escaped() {
        assert_eq!(groovy_str("a\\b\nc"), "'a\\\\b\\nc'");
    }
}
