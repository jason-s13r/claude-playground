//! The category tree.
//!
//! Indented rather than tabulated: the shape is the information, and a table
//! would flatten it away.

use std::io::{self, Write};

use cli_kit::{plural, Out, View};
use gsnz_core::Department;
use serde::Serialize;

#[derive(Serialize)]
pub struct DepartmentTree<'a> {
    pub departments: &'a [Department],
    /// How many levels to print. The trees run three deep and the third is
    /// mostly noise for someone looking for a name to pass to `browse`.
    #[serde(skip)]
    pub depth: u32,
}

impl<'a> DepartmentTree<'a> {
    pub fn new(departments: &'a [Department], depth: u32) -> DepartmentTree<'a> {
        DepartmentTree { departments, depth }
    }
}

impl View for DepartmentTree<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.departments.is_empty() {
            return writeln!(out, "No departments matched.");
        }
        let mut shown = 0usize;
        for department in self.departments {
            write_node(out, department, 0, self.depth, &mut shown)?;
        }
        writeln!(
            out,
            "\n{shown} department{}. Browse one: gsnz browse \"<name>\"",
            plural(shown)
        )
    }
}

fn write_node(
    out: &mut Out,
    department: &Department,
    level: u32,
    depth: u32,
    shown: &mut usize,
) -> io::Result<()> {
    if level >= depth {
        return Ok(());
    }
    *shown += 1;
    let indent = "  ".repeat(level as usize);
    let name = if level == 0 {
        out.heading(&department.name)
    } else {
        department.name.clone()
    };
    writeln!(out, "{indent}{name}")?;
    for child in &department.children {
        write_node(out, child, level + 1, depth, shown)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};

    fn tree() -> Vec<Department> {
        let mut deep = Department::new("Blue Milk", 2);
        deep.children.push(Department::new("Two litre", 3));
        let mut milk = Department::new("Milk", 1);
        milk.children.push(deep);
        let mut fridge = Department::new("Fridge, Deli & Eggs", 0);
        fridge.children.push(milk);
        fridge.children.push(Department::new("Eggs", 1));
        vec![fridge]
    }

    fn render(depth: u32) -> String {
        let departments = tree();
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &DepartmentTree::new(&departments, depth)).unwrap();
        out.into_string()
    }

    #[test]
    fn depth_limits_how_far_down_it_prints() {
        let one = render(1);
        assert!(one.contains("Fridge, Deli & Eggs"), "{one}");
        assert!(!one.contains("Milk"), "{one}");
        assert!(one.contains("1 department."), "{one}");

        let two = render(2);
        assert!(two.contains("Milk"), "{two}");
        assert!(!two.contains("Blue Milk"), "{two}");
        assert!(two.contains("3 departments."), "{two}");
    }

    #[test]
    fn nesting_is_shown_by_indentation() {
        let text = render(3);
        assert!(text.contains("\n  Milk\n"), "one level in: {text:?}");
        assert!(
            text.contains("\n    Blue Milk\n"),
            "two levels in: {text:?}"
        );
    }

    #[test]
    fn an_empty_tree_says_so() {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &DepartmentTree::new(&[], 2)).unwrap();
        assert_eq!(out.into_string(), "No departments matched.\n");
    }

    #[test]
    fn json_carries_the_whole_tree_regardless_of_display_depth() {
        let departments = tree();
        let mut out = Out::buffer(Format::Json);
        emit(&mut out, &DepartmentTree::new(&departments, 1)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&out.into_string()).unwrap();
        assert_eq!(value["departments"][0]["children"][0]["name"], "Milk");
        assert!(value.get("depth").is_none(), "depth is a display concern");
    }
}
