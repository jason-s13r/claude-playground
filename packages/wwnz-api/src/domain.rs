//! What this crate hands back. Woolworths-shaped, on purpose.

use serde::{Deserialize, Serialize};

/// Quantities are `f64` because a `-KGM` line is priced by the kilogram and can
/// be 1.5 of one. Everything else is a whole count.
pub fn format_quantity(q: f64) -> String {
    if is_whole(q) {
        return format!("{}", q.trunc() as i64);
    }
    // Three places is a gram; trailing zeros past that are noise.
    format!("{q:.3}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn is_whole(q: f64) -> bool {
    (q - q.trunc()).abs() < f64::EPSILON
}

/// Serialise a quantity as an integer when it is one, so a JSON consumer reads
/// `2` rather than `2.0` on every ordinary line.
fn quantity_json<S: serde::Serializer>(q: &f64, s: S) -> Result<S::Ok, S::Error> {
    if is_whole(*q) {
        s.serialize_i64(q.trunc() as i64)
    } else {
        s.serialize_f64(*q)
    }
}

/// The variant key a cart mutation needs, from whatever the user typed.
///
/// Search prints both the SKU and the variant key, and people type the SKU. A
/// bare stock code is completed to the `-EA` variant, which is what all but the
/// weighed items are; anything already carrying a unit suffix is left alone.
pub fn variant_key(sku_or_key: &str, unit: Option<&str>) -> String {
    let raw = sku_or_key.trim();
    if let Some(unit) = unit.map(str::trim).filter(|u| !u.is_empty()) {
        let stock_code = raw.split('-').next().unwrap_or(raw);
        return format!("{stock_code}-{}", unit.to_uppercase());
    }
    if raw.contains('-') {
        return raw.to_string();
    }
    format!("{raw}-EA")
}

#[derive(Clone, Debug, Serialize)]
pub struct Product {
    pub sku: String,
    /// `282768-EA` -- what cart mutations key on.
    pub variant_key: String,
    pub name: String,
    pub brand: Option<String>,
    pub unit_of_measure: Option<String>,
    pub price_cents: Option<i64>,
    pub was_price_cents: Option<i64>,
    pub unit_price_cents: Option<i64>,
    pub unit_measure: Option<String>,
    pub is_special: bool,
    pub is_club_price: bool,
    pub in_stock: Option<bool>,
    pub availability: Option<String>,
    pub department: Option<String>,
    pub store_key: Option<String>,
    /// An ad slot. A real product at a real price, marked so a reader can tell
    /// why it is at the top.
    pub sponsored: bool,
    pub image: Option<String>,
    pub url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Store {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub suburb: Option<String>,
    pub city: Option<String>,
    pub distance_km: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Category {
    pub key: String,
    pub name: String,
    pub slug: String,
    pub level: u32,
    pub children: Vec<Category>,
}

impl Category {
    /// Every node, paired with the path of names above it.
    pub fn flatten(&self) -> Vec<(Vec<String>, &Category)> {
        let mut out = Vec::new();
        self.walk(&mut Vec::new(), &mut out);
        out
    }

    fn walk<'a>(&'a self, path: &mut Vec<String>, out: &mut Vec<(Vec<String>, &'a Category)>) {
        out.push((path.clone(), self));
        path.push(self.name.clone());
        for child in &self.children {
            child.walk(path, out);
        }
        path.pop();
    }

    /// Find a department by name. Exact beats partial, shallower beats deeper.
    pub fn find(&self, needle: &str) -> Option<&Category> {
        let needle = needle.trim().to_lowercase();
        if needle.is_empty() {
            return None;
        }
        let mut all: Vec<&Category> = self.flatten().into_iter().map(|(_, c)| c).collect();
        all.sort_by_key(|c| c.level);
        all.iter()
            .find(|c| c.name.to_lowercase() == needle)
            .or_else(|| all.iter().find(|c| c.name.to_lowercase().contains(&needle)))
            .copied()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CartLine {
    pub sku: String,
    pub variant_key: String,
    pub name: String,
    pub brand: Option<String>,
    #[serde(serialize_with = "quantity_json")]
    pub quantity: f64,
    pub unit_price_cents: Option<i64>,
    pub total_cents: Option<i64>,
    pub discount_cents: Option<i64>,
    pub can_substitute: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Cart {
    pub id: Option<String>,
    pub lines: Vec<CartLine>,
    /// What the site says is in the cart, which is not `lines.len()`: it counts
    /// quantities, not distinct products.
    #[serde(serialize_with = "quantity_json")]
    pub total_items: f64,
    pub unique_products: u32,
    /// The lines alone, which is what the rows on screen add up to.
    pub items_cents: Option<i64>,
    /// Products plus fees -- the site's "order subtotal". Bigger than
    /// `items_cents` whenever a delivery or pickup fee applies.
    pub subtotal_cents: Option<i64>,
    pub to_pay_cents: Option<i64>,
    pub discount_cents: Option<i64>,
    pub store_name: Option<String>,
    pub store_id: Option<String>,
    pub fulfilment_method: Option<String>,
    /// Anything the server refused or warned about, e.g. an out-of-stock line.
    pub problems: Vec<String>,
}

/// One change to apply. Zero removes the line.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Change {
    #[serde(rename = "variantKey")]
    pub variant_key: String,
    /// Sent as an integer whenever it is one: a weighed line takes a fraction,
    /// but the schema types the rest as whole and `2.0` is not an `Int`.
    #[serde(serialize_with = "quantity_json")]
    pub quantity: f64,
}

/// Which orders to list.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize)]
pub enum Filter {
    Active,
    Past,
    #[default]
    All,
}

impl Filter {
    pub fn wire(self) -> &'static str {
        match self {
            Filter::Active => "ACTIVE",
            Filter::Past => "PAST",
            Filter::All => "ALL",
        }
    }

    pub fn parse(s: &str) -> Option<Filter> {
        match s.trim().to_lowercase().as_str() {
            "active" | "open" | "current" => Some(Filter::Active),
            "past" | "completed" | "history" => Some(Filter::Past),
            "all" => Some(Filter::All),
            _ => None,
        }
    }
}

/// One row of the history.
#[derive(Clone, Debug, Serialize)]
pub struct Order {
    pub number: String,
    pub placed_at: Option<String>,
    pub status: Option<String>,
    pub fulfilment_status: Option<String>,
    pub total_cents: Option<i64>,
    pub method: Option<String>,
    /// A pickup names a store; a delivery names an address.
    pub destination: Option<String>,
    pub slot_start: Option<String>,
    pub amendable: bool,
}

pub struct OrderPage {
    pub orders: Vec<Order>,
    pub total: u32,
    pub total_pages: u32,
}

/// A named amount that is neither a line nor the total: a delivery fee, a bag
/// fee. Reported as it arrives rather than matched against a known set.
#[derive(Clone, Debug, Serialize)]
pub struct Fee {
    /// `standardDeliveryFee`, `bagFee`.
    pub kind: String,
    pub cents: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrderLineItem {
    pub sku: String,
    /// `133211-EA` -- the variant key, for re-adding it to a cart.
    pub variant_key: String,
    pub name: String,
    pub quantity: f64,
    pub total_cents: Option<i64>,
    pub unit_price_cents: Option<i64>,
    pub saving_cents: Option<i64>,
    pub can_substitute: bool,
}

/// One order and what was in it.
///
/// Deliberately carries no customer name, email, phone or card suffix. The
/// site's own query asks for all of them; none is needed to show an order, and
/// asking would mean holding it.
#[derive(Clone, Debug, Serialize)]
pub struct OrderDetail {
    pub number: String,
    pub status: Option<String>,
    pub placed_at: Option<String>,
    pub amendable: bool,
    /// Settled at checkout. Zero on an order still in progress, which is why
    /// [`OrderDetail::total`] prefers the estimate then.
    pub total_cents: Option<i64>,
    pub estimated_total_cents: Option<i64>,
    pub discount_cents: Option<i64>,
    pub savings_cents: Option<i64>,
    pub items_cents: Option<i64>,
    pub fees: Vec<Fee>,
    pub method: Option<String>,
    pub kind: Option<String>,
    pub slot_start: Option<String>,
    pub slot_end: Option<String>,
    pub location_name: Option<String>,
    pub location_store_id: Option<String>,
    pub address: Option<String>,
    pub lines: Vec<OrderLineItem>,
}

impl OrderDetail {
    /// The number worth showing: the settled total once there is one, the
    /// estimate while the order is still being picked.
    pub fn total(&self) -> Option<i64> {
        match self.total_cents {
            Some(0) | None => self.estimated_total_cents,
            settled => settled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_stock_code_is_completed_to_the_each_variant() {
        assert_eq!(variant_key("282768", None), "282768-EA");
        assert_eq!(variant_key("282768-EA", None), "282768-EA");
        assert_eq!(variant_key("282768-KGM", None), "282768-KGM");
    }

    #[test]
    fn an_explicit_unit_replaces_whatever_suffix_was_there() {
        assert_eq!(variant_key("282768", Some("kgm")), "282768-KGM");
        assert_eq!(variant_key("282768-EA", Some("kgm")), "282768-KGM");
        assert_eq!(variant_key("282768-EA", Some("  ")), "282768-EA");
    }

    #[test]
    fn whole_quantities_do_not_print_a_decimal_point() {
        assert_eq!(format_quantity(2.0), "2");
        assert_eq!(format_quantity(1.5), "1.5");
        assert_eq!(format_quantity(0.25), "0.25");
        assert_eq!(format_quantity(1.2345), "1.234");
    }

    #[test]
    fn order_filters_accept_the_synonyms_the_site_suggests() {
        assert_eq!(Filter::parse("open"), Some(Filter::Active));
        assert_eq!(Filter::parse("history"), Some(Filter::Past));
        assert_eq!(Filter::parse("ALL"), Some(Filter::All));
        assert_eq!(Filter::parse("yesterday"), None);
        assert_eq!(Filter::Active.wire(), "ACTIVE");
    }

    #[test]
    fn an_in_progress_order_reports_its_estimate_rather_than_a_zero_total() {
        let mut order = OrderDetail {
            number: "WN1".into(),
            status: Some("IN_PROGRESS".into()),
            placed_at: None,
            amendable: false,
            total_cents: Some(0),
            estimated_total_cents: Some(43225),
            discount_cents: None,
            savings_cents: None,
            items_cents: None,
            fees: Vec::new(),
            method: None,
            kind: None,
            slot_start: None,
            slot_end: None,
            location_name: None,
            location_store_id: None,
            address: None,
            lines: Vec::new(),
        };
        assert_eq!(order.total(), Some(43225));
        order.total_cents = Some(41000);
        assert_eq!(order.total(), Some(41000), "a settled total wins");
    }

    #[test]
    fn a_category_lookup_prefers_an_exact_shallower_match() {
        let tree = Category {
            key: "root".into(),
            name: "All".into(),
            slug: "all".into(),
            level: 0,
            children: vec![
                Category {
                    key: "a".into(),
                    name: "Milkshakes".into(),
                    slug: "m".into(),
                    level: 1,
                    children: vec![],
                },
                Category {
                    key: "b".into(),
                    name: "Milk".into(),
                    slug: "milk".into(),
                    level: 1,
                    children: vec![],
                },
            ],
        };
        assert_eq!(tree.find("milk").unwrap().key, "b");
        assert_eq!(tree.find("milkshake").unwrap().key, "a");
        assert!(tree.find("").is_none());
        assert_eq!(tree.flatten().len(), 3);
    }
}
