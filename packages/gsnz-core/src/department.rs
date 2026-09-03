//! The category tree, as far as either retailer exposes one.
//!
//! Woolworths answers `GetAllCategories` with keyed, slugged nodes. Foodstuffs
//! answers `GET /v1/edge/store/{id}/categories` with a three-level tree whose
//! nodes carry a name and nothing else -- so `slug` is optional here and
//! lookups match on name. The Foodstuffs tree is also store-scoped, and mixes
//! promotional nodes ("Bonus Sticker Products") in with real departments; they
//! are listed as they arrive rather than guessed at.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Department {
    pub name: String,
    /// Woolworths has one; Foodstuffs does not.
    pub slug: Option<String>,
    /// Depth from the root, counting from zero.
    pub level: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Department>,
}

impl Department {
    pub fn new(name: impl Into<String>, level: u32) -> Department {
        Department {
            name: name.into(),
            slug: None,
            level,
            children: Vec::new(),
        }
    }

    /// Every node, paired with the path of names above it.
    pub fn flatten(&self) -> Vec<(Vec<String>, &Department)> {
        let mut out = Vec::new();
        self.walk(&mut Vec::new(), &mut out);
        out
    }

    fn walk<'a>(&'a self, path: &mut Vec<String>, out: &mut Vec<(Vec<String>, &'a Department)>) {
        out.push((path.clone(), self));
        path.push(self.name.clone());
        for child in &self.children {
            child.walk(path, out);
        }
        path.pop();
    }
}

/// Find a department by name across a forest.
///
/// An exact name beats a partial one, and a shallower match beats a deeper one:
/// typing `Milk` should find the department, not the "Milk" node buried under a
/// promotion.
pub fn find<'a>(roots: &'a [Department], needle: &str) -> Option<&'a Department> {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let mut all: Vec<&Department> = Vec::new();
    for root in roots {
        all.extend(root.flatten().into_iter().map(|(_, d)| d));
    }
    all.sort_by_key(|d| d.level);
    all.iter()
        .find(|d| d.name.to_lowercase() == needle)
        .or_else(|| all.iter().find(|d| d.name.to_lowercase().contains(&needle)))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Vec<Department> {
        let mut promo = Department::new("Bonus Sticker Products", 0);
        promo.children.push(Department::new("Milk", 1));

        let mut fridge = Department::new("Fridge, Deli & Eggs", 0);
        fridge.children.push(Department::new("Milk", 1));
        fridge.children.push(Department::new("Eggs", 1));

        vec![promo, fridge]
    }

    #[test]
    fn flatten_carries_the_path_above_each_node() {
        let roots = tree();
        let flat = roots[1].flatten();
        let eggs = flat.iter().find(|(_, d)| d.name == "Eggs").unwrap();
        assert_eq!(eggs.0, ["Fridge, Deli & Eggs"]);
    }

    #[test]
    fn an_exact_name_beats_a_partial_one() {
        let roots = vec![Department::new("Milkshakes", 0), Department::new("Milk", 1)];
        assert_eq!(find(&roots, "milk").unwrap().name, "Milk");
    }

    #[test]
    fn a_shallower_match_wins() {
        let mut roots = tree();
        roots.push(Department::new("Milk", 0));
        // Three nodes are called "Milk"; the level-0 one is the department.
        assert_eq!(find(&roots, "Milk").unwrap().level, 0);
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        assert!(find(&tree(), "").is_none());
    }
}
