//! Woolworths shapes to domain shapes, and Woolworths failures to domain
//! failures.

use gsnz_core::{
    Adjustment, Cart, CartLine, Change, Department, Error, Order, OrderLine, OrderSummary, Product,
    Quantity, RetailerId, SaleUnit, Store, StoreRef,
};
use net_kit::{AuthFault, Fault};

const ID: RetailerId = RetailerId::Woolworths;

/// A variant key ends in the unit it is sold by: `133211-EA` each,
/// `133211-KGM` by the kilogram. That suffix is the only place the distinction
/// appears on a search result.
fn unit(variant_key: &str) -> SaleUnit {
    match variant_key.rsplit('-').next() {
        Some("KGM") => SaleUnit::Weight,
        _ => SaleUnit::Each,
    }
}

fn quantity(q: f64, variant_key: &str) -> Quantity {
    match unit(variant_key) {
        SaleUnit::Weight => Quantity::kilograms(q),
        SaleUnit::Each => Quantity::units(q.round().max(0.0) as u32),
    }
}

/// A cart mutation keys on the variant, not the stock code.
///
/// Search prints both and people type the stock code, so `cart add 282848`
/// has to become `282848-EA` before it is sent. Sending the bare code is not
/// rejected as unknown -- the site answers "these items are no longer
/// available at this store", which reads like the product is gone.
pub fn change(c: &Change) -> wwnz_api::Change {
    wwnz_api::Change {
        variant_key: match c.quantity {
            // A weighed line is `-KGM`; asking for kilograms says so even when
            // what was typed was a bare code or the each variant.
            Quantity::Kilograms { .. } => wwnz_api::variant_key(&c.key, Some("kgm")),
            Quantity::Units { .. } => wwnz_api::variant_key(&c.key, None),
        },
        quantity: match c.quantity {
            Quantity::Units { count } => count as f64,
            Quantity::Kilograms { kg } => kg,
        },
    }
}

pub fn product(p: wwnz_api::Product) -> Product {
    Product {
        retailer: ID,
        // Two identifiers, and they are not interchangeable: people read and
        // type the stock code, but a cart mutation only accepts the variant key.
        key: p.variant_key.clone(),
        sku: p.sku,
        sale_unit: unit(&p.variant_key),
        name: p.name,
        brand: p.brand,
        size: p.unit_of_measure,
        price_cents: p.price_cents,
        was_price_cents: p.was_price_cents,
        unit_price_cents: p.unit_price_cents,
        unit_measure: p.unit_measure,
        // Woolworths prices promotions individually rather than as "2 for $5".
        multi_buy: None,
        is_special: p.is_special,
        is_member_price: p.is_club_price,
        in_stock: p.in_stock,
        availability: p.availability,
        department: p.department,
        image: p.image,
        url: Some(p.url),
    }
}

pub fn store(s: wwnz_api::Store) -> Store {
    Store {
        retailer: ID,
        id: s.id,
        name: s.name,
        address: s.address,
        area: s.suburb,
        city: s.city,
        distance_km: s.distance_km,
    }
}

pub fn department(c: &wwnz_api::Category) -> Department {
    Department {
        name: c.name.clone(),
        // Browsing here selects on the key rather than the name, so the slug
        // is the thing that actually works.
        slug: Some(c.key.clone()),
        level: c.level,
        children: c.children.iter().map(department).collect(),
    }
}

pub fn cart(c: wwnz_api::Cart) -> Cart {
    let mut adjustments = Vec::new();
    // The site's "order subtotal" is products plus fees, so the gap between it
    // and the lines is the fees. Reporting the difference beats inventing names
    // for charges the cart query does not itemise.
    if let (Some(items), Some(subtotal)) = (c.items_cents, c.subtotal_cents) {
        if subtotal != items {
            adjustments.push(Adjustment {
                label: "Fees".into(),
                cents: subtotal - items,
            });
        }
    }
    if let Some(discount) = c.discount_cents.filter(|d| *d != 0) {
        adjustments.push(Adjustment {
            label: "Savings".into(),
            cents: -discount,
        });
    }

    Cart {
        retailer: ID,
        store: c.store_id.map(|id| StoreRef {
            id,
            name: c.store_name.clone(),
        }),
        lines: c.lines.iter().map(line).collect(),
        // Every line the cart query returns is one the store can supply.
        unavailable: Vec::new(),
        subtotal_cents: c.items_cents,
        total_cents: c.to_pay_cents.or(c.subtotal_cents),
        adjustments,
        member: None,
        fulfilment: c.fulfilment_method,
        notes: c.problems,
        priced_at: None,
    }
}

fn line(l: &wwnz_api::CartLine) -> CartLine {
    CartLine {
        key: l.variant_key.clone(),
        sku: l.sku.clone(),
        name: l.name.clone(),
        brand: l.brand.clone(),
        quantity: quantity(l.quantity, &l.variant_key),
        unit_price_cents: l.unit_price_cents,
        total_cents: l.total_cents,
    }
}

pub fn summary(o: wwnz_api::Order) -> OrderSummary {
    OrderSummary {
        retailer: ID,
        id: o.number,
        placed_at: o.placed_at,
        total_cents: o.total_cents,
        status: o.status.or(o.fulfilment_status),
        fulfilment: o.method,
        // A summary names where it is going, not which store filled it.
        store: o.destination.map(|name| StoreRef {
            id: String::new(),
            name: Some(name),
        }),
    }
}

pub fn order_line(l: wwnz_api::OrderLineItem) -> OrderLine {
    OrderLine {
        key: l.variant_key.clone(),
        quantity: quantity(l.quantity, &l.variant_key),
        sku: l.sku,
        name: l.name,
        brand: None,
        total_cents: l.total_cents,
    }
}

pub fn order(o: wwnz_api::OrderDetail) -> Order {
    let total = o.total();
    let mut adjustments: Vec<Adjustment> = o
        .fees
        .iter()
        .filter(|f| f.cents != 0)
        .map(|f| Adjustment {
            label: fee_label(&f.kind),
            cents: f.cents,
        })
        .collect();
    if let Some(saved) = o.savings_cents.filter(|c| *c != 0) {
        adjustments.push(Adjustment {
            label: "Savings".into(),
            cents: -saved,
        });
    }

    Order {
        summary: OrderSummary {
            retailer: ID,
            id: o.number,
            placed_at: o.placed_at,
            total_cents: total,
            status: o.status,
            fulfilment: o.method,
            store: o.location_store_id.map(|id| StoreRef {
                id,
                name: o.location_name.clone(),
            }),
        },
        lines: o.lines.into_iter().map(order_line).collect(),
        address: o.address,
        timeslot: match (o.slot_start, o.slot_end) {
            (Some(start), Some(end)) => Some(format!("{start} to {end}")),
            (start, end) => start.or(end),
        },
        adjustments,
    }
}

/// `standardDeliveryFee` reads badly on a receipt. Split the camel case rather
/// than matching a fixed list, so a fee invented next month still reads.
fn fee_label(kind: &str) -> String {
    let mut out = String::new();
    for (i, c) in kind.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else if i == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The same failures, read as a *sign-in* rather than as a call with a stored
/// session. A rejection here is a wrong password, not a stale cookie -- and
/// the difference decides whether `auth refresh` is worth suggesting.
pub fn login_error(e: wwnz_api::Error) -> Error {
    use wwnz_api::Error as E;
    match e {
        E::LoginRefused { .. } | E::NoSession { .. } => Error::LoginRefused {
            retailer: ID,
            detail: e_detail(&e),
        },
        other => match other.auth() {
            Some(_) => Error::LoginRefused {
                retailer: ID,
                detail: other.to_string(),
            },
            None => error(other),
        },
    }
}

fn e_detail(e: &wwnz_api::Error) -> String {
    e.to_string()
}

pub fn error(e: wwnz_api::Error) -> Error {
    use wwnz_api::Error as E;
    match e {
        E::NotSignedIn => Error::NeedsLogin { retailer: ID },
        // A Woolworths session cookie is encrypted and only the site can mint
        // one, so "renewable" here means "we hold a password", never "we hold a
        // refresh token".
        E::SessionExpired => Error::SessionExpired {
            retailer: ID,
            renewable: true,
        },
        E::SessionUnrenewable => Error::SessionExpired {
            retailer: ID,
            renewable: false,
        },
        other => match other.auth() {
            Some(AuthFault::Missing) => Error::NeedsLogin { retailer: ID },
            Some(AuthFault::Expired) | Some(AuthFault::Rejected) => Error::SessionExpired {
                retailer: ID,
                renewable: true,
            },
            _ => Error::Upstream {
                retailer: ID,
                message: other.to_string(),
                source: Box::new(other),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variant_suffix_decides_how_a_line_is_counted() {
        assert_eq!(unit("133211-EA"), SaleUnit::Each);
        assert_eq!(unit("133211-KGM"), SaleUnit::Weight);
        assert_eq!(quantity(1.5, "133211-KGM"), Quantity::kilograms(1.5));
        assert_eq!(quantity(2.0, "133211-EA"), Quantity::units(2));
    }

    #[test]
    fn a_rejection_while_signing_in_is_not_a_stale_session() {
        // The same upstream error means different things depending on what was
        // being attempted. During a login there is no session to have expired.
        let refused = || wwnz_api::Error::LoginRefused {
            step: "password",
            detail: ": wrong password".into(),
        };
        assert!(matches!(login_error(refused()), Error::LoginRefused { .. }));
        // Whereas a genuinely lapsed session still reads as one.
        assert!(matches!(
            error(wwnz_api::Error::SessionExpired),
            Error::SessionExpired { .. }
        ));
    }

    #[test]
    fn a_typed_stock_code_is_completed_before_it_is_sent() {
        // `cart add 282848` sent as-is is answered with "no longer available
        // at this store", which reads as the product being gone rather than
        // as the key being incomplete.
        let add = |key: &str, q| {
            change(&Change {
                key: key.into(),
                quantity: q,
            })
            .variant_key
        };
        assert_eq!(add("282848", Quantity::units(1)), "282848-EA");
        assert_eq!(add("282848-EA", Quantity::units(1)), "282848-EA");
        assert_eq!(add("282848", Quantity::kilograms(0.5)), "282848-KGM");
        assert_eq!(add("282848-EA", Quantity::kilograms(0.5)), "282848-KGM");
    }

    #[test]
    fn a_fee_reads_as_words_whatever_it_is_called() {
        assert_eq!(fee_label("standardDeliveryFee"), "Standard delivery fee");
        assert_eq!(fee_label("bagFee"), "Bag fee");
        assert_eq!(fee_label("somethingNewNextYear"), "Something new next year");
    }
}
