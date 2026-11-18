//! UTF-16 helpers.

/// NUL-terminated UTF-16 copy of `s`.
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
