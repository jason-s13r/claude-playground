//! The department tree.
//!
//! `wwnz browse` takes a department the way a person says it ("Fruit & Veg"),
//! but the API selects products by an opaque category key ("9-BDB6545B"). This
//! is what turns one into the other.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Category {
    pub key: String,
    pub name: String,
    /// The path as the website spells it, e.g. `meat-poultry/beef`.
    pub slug: String,
    /// 0 is "All Departments", 1 a department, 2 an aisle, 3 a shelf.
    pub level: u32,
    pub children: Vec<Category>,
}

impl Category {
    /// Every category in this subtree, depth first, each with the path of names
    /// that reaches it.
    pub fn flatten(&self) -> Vec<(Vec<String>, &Category)> {
        let mut out = Vec::new();
        self.walk(&mut Vec::new(), &mut out);
        out
    }

    fn walk<'a>(&'a self, path: &mut Vec<String>, out: &mut Vec<(Vec<String>, &'a Category)>) {
        path.push(self.name.clone());
        out.push((path.clone(), self));
        for child in &self.children {
            child.walk(path, out);
        }
        path.pop();
    }

    /// Find the category a user meant.
    ///
    /// Exact matches win over partial ones, and a shallower match wins over a
    /// deeper one -- "Bakery" should be the department, not the "Bakery" shelf
    /// inside some other aisle. Names, slugs and keys are all accepted, since
    /// the first is what people type and the other two are what the website's
    /// own URLs carry.
    pub fn find(&self, needle: &str) -> Option<&Category> {
        let needle = needle.trim().to_lowercase();
        if needle.is_empty() {
            return None;
        }
        let all = self.flatten();

        let exact = |c: &Category| {
            c.name.to_lowercase() == needle
                || c.slug.to_lowercase() == needle
                || c.key.to_lowercase() == needle
        };
        let partial = |c: &Category| {
            c.name.to_lowercase().contains(&needle) || c.slug.to_lowercase().contains(&needle)
        };

        // `min_by_key` keeps the first of equal keys, and `flatten` is depth
        // first from the root, so ties break towards the earlier department.
        all.iter()
            .filter(|(_, c)| exact(c))
            .min_by_key(|(_, c)| c.level)
            .or_else(|| {
                all.iter()
                    .filter(|(_, c)| partial(c))
                    .min_by_key(|(_, c)| c.level)
            })
            .map(|(_, c)| *c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(key: &str, name: &str, level: u32, children: Vec<Category>) -> Category {
        Category {
            key: key.into(),
            name: name.into(),
            slug: name.to_lowercase().replace(' ', "-"),
            level,
            children,
        }
    }

    fn tree() -> Category {
        cat(
            "root",
            "All Departments",
            0,
            vec![
                cat("d1", "Bakery", 1, vec![cat("a1", "Bread", 2, vec![])]),
                cat(
                    "d2",
                    "Fridge & Deli",
                    1,
                    vec![cat(
                        "a2",
                        "Milk",
                        2,
                        vec![cat("s1", "Full Cream Milk", 3, vec![])],
                    )],
                ),
                // A shelf sharing a department's name, to prove depth breaks
                // the tie the right way.
                cat("d3", "Frozen", 1, vec![cat("a3", "Bakery", 2, vec![])]),
            ],
        )
    }

    #[test]
    fn flatten_walks_the_whole_tree_with_its_paths() {
        let tree = tree();
        let all = tree.flatten();
        assert_eq!(all.len(), 8);
        let (path, _) = all.iter().find(|(_, c)| c.key == "s1").unwrap();
        assert_eq!(
            path,
            &[
                "All Departments",
                "Fridge & Deli",
                "Milk",
                "Full Cream Milk"
            ]
        );
    }

    #[test]
    fn an_exact_name_beats_a_partial_one() {
        let t = tree();
        assert_eq!(t.find("Milk").unwrap().key, "a2");
        // "Full Cream Milk" also contains "milk", but the exact match wins.
        assert_eq!(t.find("milk").unwrap().key, "a2");
        assert_eq!(t.find("full cream").unwrap().key, "s1");
    }

    #[test]
    fn a_department_beats_a_shelf_of_the_same_name() {
        assert_eq!(tree().find("Bakery").unwrap().key, "d1");
    }

    #[test]
    fn slugs_and_keys_are_accepted_too() {
        let t = tree();
        assert_eq!(t.find("fridge-&-deli").unwrap().key, "d2");
        assert_eq!(t.find("a2").unwrap().key, "a2");
        assert!(t.find("   ").is_none());
        assert!(t.find("charcuterie").is_none());
    }
}
