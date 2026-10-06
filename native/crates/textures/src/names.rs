//! Name sanitation for created objects.

/// Prefix added when a cleaned name would start with a digit or be empty
/// (`FName` identifiers cannot).
pub(crate) const FALLBACK_PREFIX: &str = "t_";

/// Sanitize a name to use only UObject-safe characters (`[A-Za-z0-9_]`)
/// Prefixes `t_` when the result would start with a digit.
pub fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed: String = cleaned.trim_matches('_').to_owned();
    if trimmed.is_empty() || trimmed.starts_with(|c: char| c.is_ascii_digit()) {
        format!("{FALLBACK_PREFIX}{trimmed}")
    } else {
        trimmed
    }
}
