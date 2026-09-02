//! Lining the same product up across both banners.
//!
//! New World and PAK'nSAVE share one Foodstuffs catalogue, so a product carries
//! the same `productId` in both -- the SKU is the join key. Anything that only
//! one banner stocks still gets a row, with the other side blank.

use crate::domain::Product;

#[derive(Debug, Clone)]
pub struct Row {
    pub title: String,
    pub size: Option<String>,
    /// Indexed the same way as the banners passed to [`pair`].
    pub sides: Vec<Option<Product>>,
}

impl Row {
    /// Cheapest minus dearest, in cents, when both sides have a price.
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

    /// Index of the side with the lowest price, when one is strictly cheaper.
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
        let winners: Vec<usize> = priced
            .iter()
            .filter(|(_, c)| *c == min)
            .map(|(i, _)| *i)
            .collect();
        (winners.len() == 1).then(|| winners[0])
    }

    pub fn matched(&self) -> bool {
        self.sides.iter().filter(|p| p.is_some()).count() > 1
    }
}

/// Join per-banner result sets on SKU.
///
/// Rows where both banners stock the item come first, ordered by the biggest
/// price gap -- that is the reason to run the command at all. Single-banner
/// rows follow, alphabetically.
pub fn pair(sides: &[Vec<Product>]) -> Vec<Row> {
    let mut order: Vec<String> = Vec::new();
    let mut rows: std::collections::HashMap<String, Row> = std::collections::HashMap::new();

    for (index, products) in sides.iter().enumerate() {
        for product in products {
            let key = product.match_key();
            let row = rows.entry(key.clone()).or_insert_with(|| {
                order.push(key.clone());
                Row {
                    title: product.title(),
                    size: product.size.clone(),
                    sides: vec![None; sides.len()],
                }
            });
            // First banner to report a product names the row; later ones only
            // fill in a blank.
            if row.sides[index].is_none() {
                row.sides[index] = Some(product.clone());
            }
            if row.size.is_none() {
                row.size = product.size.clone();
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn product(banner: &'static str, sku: &str, name: &str, cents: Option<i64>) -> Product {
        Product {
            sku: sku.into(),
            banner,
            name: name.into(),
            brand: None,
            size: Some("2L".into()),
            price_cents: cents,
            unit_price_cents: None,
            unit_measure: None,
            multi_buy: None,
            is_special: false,
            in_stock: Some(true),
            department: None,
            image: None,
            url: "https://example.test/p".into(),
        }
    }

    #[test]
    fn joins_the_two_banners_on_sku() {
        let rows = pair(&[
            vec![product("newworld", "A-EA-000", "Milk", Some(450))],
            vec![product("paknsave", "a-ea-000", "Milk", Some(399))],
        ]);
        assert_eq!(rows.len(), 1, "same sku should collapse to one row");
        assert!(rows[0].matched());
        assert_eq!(rows[0].saving(), Some(51));
        assert_eq!(rows[0].cheapest(), Some(1));
    }

    #[test]
    fn keeps_products_only_one_banner_stocks() {
        let rows = pair(&[
            vec![product("newworld", "A", "Only NW", Some(100))],
            vec![product("paknsave", "B", "Only PNS", Some(200))],
        ]);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| !r.matched()));
        assert!(rows.iter().all(|r| r.saving().is_none()));
    }

    #[test]
    fn matched_rows_sort_ahead_of_unmatched_and_by_biggest_gap() {
        let rows = pair(&[
            vec![
                product("newworld", "SMALL", "Small gap", Some(210)),
                product("newworld", "BIG", "Big gap", Some(900)),
                product("newworld", "SOLO", "Solo", Some(100)),
            ],
            vec![
                product("paknsave", "SMALL", "Small gap", Some(200)),
                product("paknsave", "BIG", "Big gap", Some(500)),
            ],
        ]);
        let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, vec!["Big gap", "Small gap", "Solo"]);
    }

    #[test]
    fn an_equal_price_has_no_winner() {
        let rows = pair(&[
            vec![product("newworld", "A", "Milk", Some(400))],
            vec![product("paknsave", "A", "Milk", Some(400))],
        ]);
        assert_eq!(rows[0].saving(), Some(0));
        assert_eq!(rows[0].cheapest(), None);
    }
}
