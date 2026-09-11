//! The category tree, flattened to what a person can act on.

use std::io::{self, Write};

use bgnz_api::Category;
use cli_kit::{table, Out, View};
use serde::Serialize;

#[derive(Serialize)]
pub struct CategoryTree<'a> {
    pub categories: &'a [Category],
    pub banner: String,
}

impl<'a> CategoryTree<'a> {
    pub fn new(categories: &'a [Category], banner: bgnz_api::Banner) -> CategoryTree<'a> {
        CategoryTree {
            categories,
            banner: banner.name().to_string(),
        }
    }
}

impl View for CategoryTree<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.categories.is_empty() {
            return writeln!(out, "No categories found.");
        }
        let mut t = table(&["ID", "Category", "Path", "Children"]);
        for c in self.categories {
            t.add_row(vec![
                // The numeric id, because that is what `browse` takes -- the
                // base64 uid works too, and nobody wants to type it.
                c.id.clone(),
                format!("{}{}", "  ".repeat((c.level.max(2) - 2) as usize), c.name),
                c.url_path.clone().unwrap_or_else(|| "—".into()),
                c.children.to_string(),
            ]);
        }
        writeln!(out, "{t}")?;
        super::write_count(
            out,
            self.categories.len(),
            "category",
            Some("List one: `bgnz browse <id>`."),
        )
    }
}
