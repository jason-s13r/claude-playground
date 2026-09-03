//! Tables, and the two string helpers every listing needs.

use comfy_table::{presets, ContentArrangement, Table};

/// The house table: light rules, and columns that size to the terminal rather
/// than to the widest cell.
pub fn table(headers: &[&str]) -> Table {
    let mut t = Table::new();
    t.load_preset(presets::UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic);
    if !headers.is_empty() {
        t.set_header(headers.iter().map(|h| h.to_string()));
    }
    t
}

/// `""` or `"s"`. Saves every caller an inline `if`.
pub fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Name a thing under its owner, without stuttering when the name already
/// says it.
///
/// Store names usually already carry the chain -- "New World Thorndon" -- so
/// prefixing the chain again reads badly.
pub fn qualified(owner: &str, name: Option<&str>) -> String {
    match name {
        Some(name) if name.to_lowercase().contains(&owner.to_lowercase()) => name.to_string(),
        Some(name) => format!("{owner} — {name}"),
        None => owner.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals() {
        assert_eq!(plural(1), "");
        assert_eq!(plural(0), "s");
        assert_eq!(plural(2), "s");
    }

    #[test]
    fn a_name_that_already_says_the_owner_does_not_repeat_it() {
        assert_eq!(
            qualified("New World", Some("New World Thorndon")),
            "New World Thorndon"
        );
        assert_eq!(qualified("New World", Some("4147")), "New World — 4147");
        assert_eq!(qualified("PAK'nSAVE", None), "PAK'nSAVE");
    }

    #[test]
    fn the_owner_check_ignores_case() {
        assert_eq!(
            qualified("Woolworths", Some("WOOLWORTHS Regent")),
            "WOOLWORTHS Regent"
        );
    }

    #[test]
    fn a_table_renders_its_headers() {
        let mut t = table(&["Store", "Price"]);
        t.add_row(vec!["Thorndon", "$4.29"]);
        let text = t.to_string();
        assert!(text.contains("Store"));
        assert!(text.contains("$4.29"));
    }
}
