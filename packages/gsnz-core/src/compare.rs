//! Lining the same product up across retailers.
//!
//! New World and PAK'nSAVE share one Foodstuffs catalogue, so the SKU is a
//! reliable join key between them. Woolworths is a different company with its
//! own product codes, and nothing joins those two catalogues exactly -- so the
//! second tier matches on brand, name and size, and every row records which
//! tier produced it. A comparison that silently equates two different two-litre
//! milks is a wrong-price bug, which is the worst kind this tool can have.

use serde::Serialize;

use crate::product::Product;

/// How two products were decided to be the same thing.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum MatchKey {
    /// Same catalogue, same product code. Exact.
    Catalogue { family: &'static str, sku: String },
    /// Different catalogues, similar description. Best effort.
    Normalised(String),
}

/// How confident a row is, for the caller to render honestly.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Match {
    Exact,
    Normalised,
}

#[derive(Clone, Debug, Serialize)]
pub struct Row {
    pub title: String,
    pub size: Option<String>,
    /// Indexed the same way as the sides passed to [`pair`].
    pub sides: Vec<Option<Product>>,
    #[serde(rename = "match")]
    pub match_kind: Match,
}

impl Row {
    /// Dearest minus cheapest, in cents, when at least two sides have a price.
    pub fn saving(&self) -> Option<i64> {
        let mut prices: Vec<i64> = self
            .sides
            .iter()
            .filter_map(|p| p.as_ref().and_then(|p| p.price_cents))
            .collect();
        if prices.len() < 2 {
            return None;
        }
        prices.sort_unstable();
        Some(prices[prices.len() - 1] - prices[0])
    }

    /// Index of the one cheapest side, when exactly one is cheapest.
    pub fn cheapest(&self) -> Option<usize> {
        let priced: Vec<(usize, i64)> = self
            .sides
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.as_ref().and_then(|p| p.price_cents).map(|c| (i, c)))
            .collect();
        if priced.len() < 2 {
            return None;
        }
        let min = priced.iter().map(|(_, c)| *c).min()?;
        let mut winners = priced.iter().filter(|(_, c)| *c == min);
        let first = winners.next()?.0;
        winners.next().is_none().then_some(first)
    }

    pub fn matched(&self) -> bool {
        self.sides.iter().filter(|p| p.is_some()).count() > 1
    }
}

/// Words that say nothing about which product this is.
const NOISE: [&str; 8] = ["the", "and", "with", "of", "a", "pack", "value", "nz"];

/// Fold a product down to something comparable across catalogues.
///
/// Lowercase alphanumerics, noise words dropped, and the size canonicalised so
/// `2L`, `2 litre` and `2000ml` agree.
pub fn normalise(product: &Product) -> String {
    let mut words: Vec<String> = product
        .title()
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !NOISE.contains(w))
        .map(str::to_string)
        .collect();
    words.sort();
    words.dedup();
    match product.size.as_deref().and_then(canonical_size) {
        Some(size) => format!("{} {size}", words.join(" ")),
        None => words.join(" "),
    }
}

/// `2L` and `2 litre` and `2000ml` all become `2000ml`.
///
/// Returns `None` for a size that is not a plain measurement, so an unparseable
/// size never makes two different products look equal.
pub fn canonical_size(size: &str) -> Option<String> {
    let s = size.trim().to_lowercase().replace(' ', "");
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (number, unit) = s.split_at(split);
    let value: f64 = number.parse().ok()?;
    let (scale, base) = match unit.trim_end_matches('s') {
        "l" | "litre" | "liter" => (1000.0, "ml"),
        "ml" | "millilitre" | "milliliter" => (1.0, "ml"),
        "kg" | "kilo" | "kilogram" => (1000.0, "g"),
        "g" | "gram" => (1.0, "g"),
        _ => return None,
    };
    Some(format!("{}{base}", (value * scale).round() as i64))
}

/// Join per-retailer result sets into comparison rows.
///
/// Exact catalogue matches are made first so the Foodstuffs pairing is never
/// weakened by the fuzzy pass. Rows with more than one side come first, ordered
/// by the biggest price gap -- that is the reason to run the command at all --
/// then single-retailer rows alphabetically.
pub fn pair(sides: &[Vec<Product>], allow_normalised: bool) -> Vec<Row> {
    use std::collections::HashMap;

    let mut order: Vec<MatchKey> = Vec::new();
    let mut rows: HashMap<MatchKey, Row> = HashMap::new();

    let mut insert = |key: MatchKey, index: usize, product: &Product, kind: Match| {
        let row = rows.entry(key.clone()).or_insert_with(|| {
            order.push(key);
            Row {
                title: product.title(),
                size: product.size.clone(),
                sides: vec![None; sides.len()],
                match_kind: kind,
            }
        });
        // The first retailer to report a product names the row; later ones only
        // fill a blank, so a fuzzy hit cannot rewrite an exact row's title.
        if row.sides[index].is_none() {
            row.sides[index] = Some(product.clone());
            if kind == Match::Normalised {
                row.match_kind = Match::Normalised;
            }
        }
        if row.size.is_none() {
            row.size = product.size.clone();
        }
    };

    for (index, products) in sides.iter().enumerate() {
        for product in products {
            match product.retailer.catalogue() {
                Some(family) => insert(
                    MatchKey::Catalogue {
                        family,
                        sku: product.sku.to_lowercase(),
                    },
                    index,
                    product,
                    Match::Exact,
                ),
                // Nothing shares this catalogue, so an exact join is impossible
                // and the product gets its own row unless the fuzzy pass runs.
                None => insert(
                    MatchKey::Normalised(normalise(product)),
                    index,
                    product,
                    Match::Exact,
                ),
            }
        }
    }

    if allow_normalised {
        merge_across_catalogues(&mut order, &mut rows);
    }

    let mut out: Vec<Row> = order.into_iter().filter_map(|k| rows.remove(&k)).collect();
    out.sort_by(|a, b| {
        b.matched()
            .cmp(&a.matched())
            .then_with(|| b.saving().unwrap_or(-1).cmp(&a.saving().unwrap_or(-1)))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    out
}

/// Fold single-catalogue rows into each other when their descriptions agree.
fn merge_across_catalogues(
    order: &mut Vec<MatchKey>,
    rows: &mut std::collections::HashMap<MatchKey, Row>,
) {
    use std::collections::HashMap;

    // Only rows that still have an empty side can absorb another.
    let mut by_description: HashMap<String, MatchKey> = HashMap::new();
    let mut merged: Vec<MatchKey> = Vec::new();

    for key in order.iter() {
        let Some(row) = rows.get(key) else { continue };
        let Some(sample) = row.sides.iter().flatten().next() else {
            continue;
        };
        let description = normalise(sample);
        match by_description.get(&description) {
            Some(into) if into != key => {
                let (Some(source), Some(target)) = (rows.get(key).cloned(), rows.get_mut(into))
                else {
                    continue;
                };
                let mut absorbed = false;
                for (slot, product) in target.sides.iter_mut().zip(source.sides) {
                    if slot.is_none() {
                        if let Some(p) = product {
                            *slot = Some(p);
                            absorbed = true;
                        }
                    }
                }
                if absorbed {
                    target.match_kind = Match::Normalised;
                    if target.size.is_none() {
                        target.size = source.size;
                    }
                    merged.push(key.clone());
                }
            }
            _ => {
                by_description.insert(description, key.clone());
            }
        }
    }

    for key in merged {
        rows.remove(&key);
        order.retain(|k| k != &key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product::SaleUnit;
    use crate::retailer::RetailerId;

    fn product(
        retailer: RetailerId,
        sku: &str,
        name: &str,
        size: &str,
        cents: Option<i64>,
    ) -> Product {
        Product {
            retailer,
            sku: sku.into(),
            key: sku.into(),
            name: name.into(),
            brand: None,
            size: Some(size.into()),
            price_cents: cents,
            was_price_cents: None,
            unit_price_cents: None,
            unit_measure: None,
            sale_unit: SaleUnit::Each,
            multi_buy: None,
            is_special: false,
            is_member_price: false,
            in_stock: Some(true),
            availability: None,
            department: None,
            image: None,
            url: None,
        }
    }

    #[test]
    fn canonicalises_the_units_people_write() {
        assert_eq!(canonical_size("2L").as_deref(), Some("2000ml"));
        assert_eq!(canonical_size("2 litre").as_deref(), Some("2000ml"));
        assert_eq!(canonical_size("2000ml").as_deref(), Some("2000ml"));
        assert_eq!(canonical_size("1kg").as_deref(), Some("1000g"));
        assert_eq!(canonical_size("500g").as_deref(), Some("500g"));
        assert_eq!(canonical_size("1.5L").as_deref(), Some("1500ml"));
    }

    #[test]
    fn refuses_a_size_it_cannot_read() {
        // Better no match than a wrong one: "6 pack" is not a measurement.
        assert_eq!(canonical_size("6 pack"), None);
        assert_eq!(canonical_size(""), None);
        assert_eq!(canonical_size("each"), None);
    }

    #[test]
    fn joins_the_foodstuffs_banners_on_sku() {
        let rows = pair(
            &[
                vec![product(
                    RetailerId::NewWorld,
                    "A-EA-000",
                    "Milk",
                    "2L",
                    Some(450),
                )],
                vec![product(
                    RetailerId::PaknSave,
                    "a-ea-000",
                    "Milk",
                    "2L",
                    Some(399),
                )],
            ],
            false,
        );
        assert_eq!(rows.len(), 1, "same sku should collapse to one row");
        assert_eq!(rows[0].match_kind, Match::Exact);
        assert_eq!(rows[0].saving(), Some(51));
        assert_eq!(rows[0].cheapest(), Some(1));
    }

    #[test]
    fn woolworths_stays_separate_without_the_normalised_pass() {
        let rows = pair(
            &[
                vec![product(
                    RetailerId::NewWorld,
                    "A-EA-000",
                    "Anchor Blue Milk",
                    "2L",
                    Some(450),
                )],
                vec![product(
                    RetailerId::Woolworths,
                    "282768",
                    "Anchor Blue Milk",
                    "2L",
                    Some(520),
                )],
            ],
            false,
        );
        assert_eq!(
            rows.len(),
            2,
            "different catalogues cannot be joined exactly"
        );
        assert!(rows.iter().all(|r| !r.matched()));
    }

    #[test]
    fn the_normalised_pass_attaches_woolworths_and_says_so() {
        let rows = pair(
            &[
                vec![product(
                    RetailerId::NewWorld,
                    "A-EA-000",
                    "Anchor Blue Milk",
                    "2L",
                    Some(450),
                )],
                vec![product(
                    RetailerId::Woolworths,
                    "282768",
                    "Anchor Blue Milk",
                    "2 litre",
                    Some(520),
                )],
            ],
            true,
        );
        assert_eq!(rows.len(), 1);
        assert!(rows[0].matched());
        assert_eq!(
            rows[0].match_kind,
            Match::Normalised,
            "a fuzzy join must be labelled"
        );
        assert_eq!(rows[0].cheapest(), Some(0));
    }

    #[test]
    fn a_foodstuffs_pairing_stays_exact_even_with_the_fuzzy_pass_on() {
        let rows = pair(
            &[
                vec![product(
                    RetailerId::NewWorld,
                    "A-EA-000",
                    "Milk",
                    "2L",
                    Some(450),
                )],
                vec![product(
                    RetailerId::PaknSave,
                    "A-EA-000",
                    "Milk",
                    "2L",
                    Some(399),
                )],
            ],
            true,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].match_kind, Match::Exact);
    }

    #[test]
    fn different_sizes_do_not_merge() {
        let rows = pair(
            &[
                vec![product(
                    RetailerId::NewWorld,
                    "A",
                    "Anchor Blue Milk",
                    "2L",
                    Some(450),
                )],
                vec![product(
                    RetailerId::Woolworths,
                    "B",
                    "Anchor Blue Milk",
                    "1L",
                    Some(300),
                )],
            ],
            true,
        );
        assert_eq!(rows.len(), 2, "a 1L is not a cheaper 2L");
    }

    #[test]
    fn matched_rows_sort_ahead_of_unmatched_and_by_biggest_gap() {
        let rows = pair(
            &[
                vec![
                    product(RetailerId::NewWorld, "SMALL", "Small gap", "1L", Some(210)),
                    product(RetailerId::NewWorld, "BIG", "Big gap", "1L", Some(900)),
                    product(RetailerId::NewWorld, "SOLO", "Solo", "1L", Some(100)),
                ],
                vec![
                    product(RetailerId::PaknSave, "SMALL", "Small gap", "1L", Some(200)),
                    product(RetailerId::PaknSave, "BIG", "Big gap", "1L", Some(500)),
                ],
            ],
            false,
        );
        let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Big gap", "Small gap", "Solo"]);
    }

    #[test]
    fn an_equal_price_has_no_winner() {
        let rows = pair(
            &[
                vec![product(RetailerId::NewWorld, "A", "Milk", "2L", Some(400))],
                vec![product(RetailerId::PaknSave, "A", "Milk", "2L", Some(400))],
            ],
            false,
        );
        assert_eq!(rows[0].saving(), Some(0));
        assert_eq!(rows[0].cheapest(), None);
    }
}
