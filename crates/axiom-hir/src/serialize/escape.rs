//! Re-escape decoded string values for the HIR dump, so a literal that was
//! decoded (e.g. a real newline) prints back as a readable single-line token
//! (`"\n"`). The inverse of `lower::decode_str_escapes`.

/// Render control characters and quotes as their escape sequences.
pub(super) fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}
