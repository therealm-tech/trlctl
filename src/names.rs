/// Whether `name` is a lowercase DNS label as SPEC.md §5 requires for
/// `<app>`, `<env>`, `<cluster>` and `<instance>`.
#[must_use]
pub fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    bytes.len() <= 63
        && first.is_ascii_alphanumeric()
        && last.is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

#[cfg(test)]
mod tests {
    use super::is_valid_name;

    #[test]
    fn accepts_lowercase_labels() {
        for name in ["a", "prod-eu-1", "0x", "a-b-c", &"a".repeat(63)] {
            assert!(is_valid_name(name), "{name}");
        }
    }

    #[test]
    fn rejects_everything_else() {
        for name in [
            "",
            "-a",
            "a-",
            "Prod",
            "prod_eu",
            "prod.eu",
            "café",
            &"a".repeat(64),
        ] {
            assert!(!is_valid_name(name), "{name}");
        }
    }
}
