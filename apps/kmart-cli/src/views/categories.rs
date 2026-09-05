//! The category tree.

use std::io::{self, Write};

use cli_kit::{serde_json, Out, View};
use kmart_api::Category;
use serde::Serialize;

#[derive(Serialize)]
pub struct CategoryTree<'a> {
    pub categories: &'a [Category],
    pub depth: u32,
}

impl View for CategoryTree<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.categories.is_empty() {
            return writeln!(out, "No categories found.");
        }
        for category in self.categories {
            write_branch(out, category, 0, self.depth)?;
        }
        let total: usize = self.categories.iter().map(|c| c.walk().len()).sum();
        super::write_irregular_count(
            out,
            total,
            "category",
            "categories",
            Some("Browse one with `kmart browse <ID>`."),
        )
    }

    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

fn write_branch(out: &mut Out, category: &Category, level: u32, depth: u32) -> io::Result<()> {
    let indent = "  ".repeat(level as usize);
    let count = match category.count {
        Some(n) => format!(" ({n})"),
        None => String::new(),
    };
    // The id is what `browse` takes, so it is printed rather than left to be
    // looked up -- and dimmed, because the name is what a person reads.
    writeln!(
        out,
        "{indent}{}{} {}",
        category.name,
        out.dim(&count),
        out.dim(&category.id)
    )?;
    if level + 1 < depth {
        for child in &category.children {
            write_branch(out, child, level + 1, depth)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};

    fn category(id: &str, name: &str, children: Vec<Category>) -> Category {
        Category {
            id: id.into(),
            name: name.into(),
            path: Some(format!("/{name}/")),
            count: Some(10),
            children,
        }
    }

    fn render(categories: &[Category], depth: u32) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &CategoryTree { categories, depth }).unwrap();
        out.into_string()
    }

    #[test]
    fn a_tree_is_indented_and_carries_the_id_browse_takes() {
        let tree = vec![category(
            "aaa",
            "Home & Living",
            vec![category("bbb", "Cleaning", Vec::new())],
        )];
        let text = render(&tree, 2);
        assert!(text.contains("Home & Living"), "{text}");
        assert!(text.contains("  Cleaning"), "indented: {text}");
        assert!(text.contains("bbb"), "the id is what browse takes: {text}");
        assert!(text.contains("2 categories"), "{text}");
    }

    #[test]
    fn depth_stops_the_walk_without_hiding_the_count() {
        let tree = vec![category(
            "aaa",
            "Home & Living",
            vec![category("bbb", "Cleaning", Vec::new())],
        )];
        let text = render(&tree, 1);
        assert!(!text.contains("Cleaning"), "{text}");
        // The count is of what exists, not of what was printed.
        assert!(text.contains("2 categories"), "{text}");
    }

    #[test]
    fn one_category_is_not_pluralised_into_a_non_word() {
        let text = render(&[category("aaa", "Toys", Vec::new())], 1);
        assert!(text.contains("1 category."), "{text}");
        assert!(!text.contains("categorys"), "{text}");
    }

    #[test]
    fn an_empty_tree_says_so() {
        assert!(render(&[], 2).contains("No categories found."));
    }
}
