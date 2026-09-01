//! Rendering the department tree.

use crate::domain::category::Category;
use crate::output::plural;

/// Print the tree down to `depth`, indented by level.
///
/// The root ("All Departments") is not printed: it is not something anyone
/// browses, and every line would be indented under it for no gain.
pub fn print_categories(root: &Category, query: Option<&str>, depth: u32) {
    let mut shown = 0usize;
    for (path, category) in root.flatten() {
        if category.level == 0 || category.level > depth {
            continue;
        }
        if let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) {
            // Match anywhere on the path, so a search for a department also
            // turns up the aisles inside it.
            let hay = path.join(" ").to_lowercase();
            if !hay.contains(&q.to_lowercase()) {
                continue;
            }
        }
        let indent = "  ".repeat((category.level - 1) as usize);
        println!("{indent}{}  ({})", category.name, category.key);
        shown += 1;
    }

    if shown == 0 {
        match query {
            Some(q) => println!("No department matches '{q}'."),
            None => println!("No departments returned."),
        }
        return;
    }
    println!(
        "\n{shown} department{}. Browse one: wwnz browse <name>",
        plural(shown)
    );
}
