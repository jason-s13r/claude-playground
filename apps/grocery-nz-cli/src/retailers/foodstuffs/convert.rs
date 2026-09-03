//! Foodstuffs shapes to domain shapes, and Foodstuffs failures to domain
//! failures.
//!
//! This is the only file that constructs a [`gsnz_core::Error`] out of an
//! `fsnz_api::Error`, which is what keeps the classification in one place
//! instead of spread across a dozen `?`s.

use fsnz_api::{Banner, SaleType};
use gsnz_core::{
    Adjustment, Cart, CartLine, Change, Department, Error, Order, OrderLine, OrderSummary, Product,
    Quantity, RetailerId, SaleUnit, Store, StoreRef,
};
use net_kit::{AuthFault, Fault};

pub fn banner(id: RetailerId) -> Option<Banner> {
    match id {
        RetailerId::NewWorld => Some(Banner::NewWorld),
        RetailerId::PaknSave => Some(Banner::PaknSave),
        RetailerId::Woolworths => None,
    }
}

fn unit(sale_type: SaleType) -> SaleUnit {
    match sale_type {
        SaleType::Units => SaleUnit::Each,
        SaleType::Weight => SaleUnit::Weight,
    }
}

/// A Foodstuffs weight line counts grams; the domain counts kilograms, because
/// that is the unit the price is quoted in.
fn quantity(count: u32, sale_type: SaleType) -> Quantity {
    match sale_type {
        SaleType::Units => Quantity::units(count),
        SaleType::Weight => Quantity::kilograms(count as f64 / 1000.0),
    }
}

pub fn change(id: RetailerId, c: &Change) -> fsnz_api::Change {
    let (quantity, sale_type) = match c.quantity {
        Quantity::Units { count } => (count, SaleType::Units),
        // Back to grams, and rounded rather than truncated: 0.35kg typed by a
        // person must not become 349g.
        Quantity::Kilograms { kg } => ((kg * 1000.0).round().max(0.0) as u32, SaleType::Weight),
    };
    let _ = id;
    fsnz_api::Change {
        sku: c.key.clone(),
        quantity,
        sale_type,
    }
}

pub fn product(id: RetailerId, p: fsnz_api::Product) -> Product {
    Product {
        retailer: id,
        // Foodstuffs uses one identifier for looking a product up and for
        // changing a cart line, so both fields carry it.
        key: p.sku.clone(),
        sku: p.sku.clone(),
        name: p.name,
        brand: p.brand,
        size: p.size,
        price_cents: p.price_cents,
        // Foodstuffs sends the promotion already rendered ("2 for $5.00") and
        // never the price it was before, so there is nothing honest to put here.
        was_price_cents: None,
        unit_price_cents: p.unit_price_cents,
        unit_measure: p.unit_measure,
        sale_unit: unit(SaleType::infer(&p.sku)),
        multi_buy: p.multi_buy,
        is_special: p.is_special,
        is_member_price: false,
        in_stock: p.in_stock,
        availability: None,
        department: p.department,
        image: p.image,
        url: Some(p.url),
    }
}

pub fn store(id: RetailerId, s: fsnz_api::Store) -> Store {
    Store {
        retailer: id,
        id: s.id,
        name: s.name,
        address: s.address,
        area: s.region,
        // Foodstuffs answers the store list with no city field and no
        // coordinates, so there is no distance to report.
        city: None,
        distance_km: None,
    }
}

pub fn department(c: fsnz_api::Category, level: u32) -> Department {
    Department {
        name: c.name,
        // Name only: the Foodstuffs tree carries no key, and the search filter
        // it feeds matches on the name anyway.
        slug: None,
        level,
        children: c
            .children
            .into_iter()
            .map(|child| department(child, level + 1))
            .collect(),
    }
}

pub fn cart(id: RetailerId, c: fsnz_api::Cart) -> Cart {
    let total = c.estimated_total_cents();
    // Named amounts rather than named fields: a fee Foodstuffs invents next
    // month becomes an extra row instead of a number that quietly vanishes.
    let adjustments = [
        ("Service fee", c.service_fee_cents),
        ("Bag fee", c.bag_fee_cents),
        ("Promotions", -c.promo_discount_cents),
        ("Club discount", -c.subscription_discount_cents),
    ]
    .into_iter()
    .filter(|(_, cents)| *cents != 0)
    .map(|(label, cents)| Adjustment {
        label: label.to_string(),
        cents,
    })
    .collect();

    Cart {
        retailer: id,
        store: c.store_id.map(|store_id| StoreRef {
            id: store_id,
            name: c.store_name.clone(),
        }),
        lines: c.items.iter().map(line).collect(),
        unavailable: c.unavailable.iter().map(line).collect(),
        subtotal_cents: Some(c.subtotal_cents),
        total_cents: Some(total),
        adjustments,
        member: Some(c.club_member),
        fulfilment: None,
        notes: Vec::new(),
        priced_at: c.priced_at,
    }
}

fn line(i: &fsnz_api::CartItem) -> CartLine {
    CartLine {
        key: i.sku.clone(),
        sku: i.sku.clone(),
        name: i.name.clone(),
        brand: None,
        quantity: quantity(i.quantity, i.sale_type),
        unit_price_cents: None,
        total_cents: Some(i.line_total_cents),
    }
}

pub fn summary(id: RetailerId, o: fsnz_api::OrderSummary) -> OrderSummary {
    let source = o.resolved_source();
    OrderSummary {
        retailer: id,
        id: o.id,
        placed_at: o.placed_at,
        total_cents: o.total_cents,
        status: None,
        // An in-store receipt and an online order are different enough that the
        // distinction belongs on screen; there is no other status on a summary.
        fulfilment: Some(source.label().to_string()),
        store: o.store_id.map(|store_id| StoreRef {
            id: store_id,
            name: o.store_name,
        }),
    }
}

pub fn order_line(l: fsnz_api::OrderLine) -> OrderLine {
    OrderLine {
        key: l.sku.clone(),
        sku: l.sku,
        name: l.name,
        brand: l.brand,
        quantity: quantity(l.quantity, l.sale_type),
        total_cents: Some(l.line_total_cents),
    }
}

pub fn order(id: RetailerId, o: fsnz_api::Order) -> Order {
    let mut summary = summary(id, o.summary);
    if o.status.is_some() {
        summary.status = o.status;
    }
    if let Some(fulfilment) = o.fulfilment {
        summary.fulfilment = Some(fulfilment);
    }
    let adjustments = [
        ("Service fee", o.service_fee_cents),
        ("Bag fee", o.bag_fee_cents),
    ]
    .into_iter()
    .filter(|(_, cents)| *cents != 0)
    .map(|(label, cents)| Adjustment {
        label: label.to_string(),
        cents,
    })
    .collect();

    Order {
        summary,
        lines: o.lines.into_iter().map(order_line).collect(),
        address: o.address,
        timeslot: o.timeslot,
        adjustments,
    }
}

/// The one place a Foodstuffs failure becomes something the user reads.
///
/// `renewable` is the load-bearing bit: a stored login with a refresh token can
/// be renewed silently, and one without cannot, and telling the user to run
/// `auth refresh` when it will not work wastes their time.
/// The same failures, read as a *sign-in*. `Unauthorised` on an ordinary call
/// means the token went stale and renewing is the fix; during a login it means
/// Club Plus did not accept what was typed, and there is nothing to renew.
pub fn login_error(id: RetailerId, e: fsnz_api::Error) -> Error {
    use fsnz_api::Error as E;
    match e {
        E::VerificationRequired { .. } => Error::LoginRefused {
            retailer: id,
            detail: "the verification code was not accepted".into(),
        },
        other => match other.auth() {
            Some(_) => Error::LoginRefused {
                retailer: id,
                detail: other.to_string(),
            },
            None => error(id, other),
        },
    }
}

pub fn error(id: RetailerId, e: fsnz_api::Error) -> Error {
    use fsnz_api::Error as E;
    match e {
        E::NotLoggedIn => Error::NeedsLogin { retailer: id },
        E::SessionUnrenewable => Error::SessionExpired {
            retailer: id,
            renewable: false,
        },
        E::RefreshRejected => Error::SessionExpired {
            retailer: id,
            renewable: false,
        },
        E::CartStoreUnbound => Error::NoStore { retailer: id },
        other => match other.auth() {
            Some(AuthFault::Missing) => Error::NeedsLogin { retailer: id },
            Some(AuthFault::Expired) | Some(AuthFault::Rejected) => Error::SessionExpired {
                retailer: id,
                renewable: true,
            },
            _ => Error::Upstream {
                retailer: id,
                message: other.to_string(),
                source: Box::new(other),
            },
        },
    }
}
