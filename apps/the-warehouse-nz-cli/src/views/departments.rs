//! The department tree.

use std::io::{self, Write};

use cli_kit::{Out, View};
use serde::Serialize;
use twlnz_api::Category;

#[derive(Serialize)]
pub struct DepartmentTree<'a> {
    pub departments: &'a [Category],
}

impl View for DepartmentTree<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.departments.is_empty() {
            return writeln!(out, "No departments found.");
        }
        for department in self.departments {
            write_node(out, department, 0)?;
        }
        let total: usize = self.departments.iter().map(Category::count).sum();
        super::write_count(
            out,
            total,
            "department",
            Some("Browse one: `twlnz browse <id>`."),
        )
    }
}

fn write_node(out: &mut Out, category: &Category, depth: usize) -> io::Result<()> {
    let indent = "  ".repeat(depth);
    // The id, not the name, because the id is what `browse` takes -- and the
    // two are not derivable from each other.
    writeln!(out, "{indent}{}  {}", category.name, out.dim(&category.id))?;
    for child in &category.children {
        write_node(out, child, depth + 1)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};

    #[test]
    fn every_line_carries_the_id_browse_takes() {
        let departments = vec![Category {
            id: "toysbaby".into(),
            name: "Toys & Baby".into(),
            path: Some("/c/toys-baby".into()),
            children: vec![Category {
                id: "toysbaby-babytoddler".into(),
                name: "Baby & Toddler".into(),
                path: None,
                children: vec![],
            }],
        }];
        let mut out = Out::buffer(Format::Text);
        emit(
            &mut out,
            &DepartmentTree {
                departments: &departments,
            },
        )
        .unwrap();
        let text = out.into_string();
        assert!(text.contains("Toys & Baby  toysbaby"), "{text}");
        assert!(
            text.contains("  Baby & Toddler  toysbaby-babytoddler"),
            "{text}"
        );
        assert!(
            text.contains("2 departments."),
            "the count is of nodes: {text}"
        );
    }
}
