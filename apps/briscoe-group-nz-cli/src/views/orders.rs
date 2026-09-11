//! What has been bought.
//!
//! Two histories, rendered apart because they *are* apart: online orders come
//! out of the storefront with a tracking link, and in-store receipts come out
//! of SAP with a shop and a receipt reference. Neither is a page of the other.

use std::io::{self, Write};

use bgnz_api::{OrderDetail, OrderPage, ReceiptPage};
use cli_kit::{table, Out, View};
use serde::Serialize;

#[derive(Serialize)]
pub struct OrderList<'a> {
    pub page: &'a OrderPage,
    pub banner: String,
}

impl<'a> OrderList<'a> {
    pub fn new(page: &'a OrderPage, banner: bgnz_api::Banner) -> OrderList<'a> {
        OrderList {
            page,
            banner: banner.name().to_string(),
        }
    }
}

impl View for OrderList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let p = self.page;
        if p.orders.is_empty() {
            return writeln!(
                out,
                "No online {} orders. In-shop receipts are a separate history: \
                 `bgnz orders receipts`.",
                self.banner
            );
        }
        let mut t = table(&["Order", "Date", "Status", "Fulfilment", "Total"]);
        for o in &p.orders {
            t.add_row(vec![
                o.number.clone(),
                o.date.clone().unwrap_or_else(|| "—".into()),
                o.status.clone().unwrap_or_else(|| "—".into()),
                o.pickup_store
                    .clone()
                    .or_else(|| o.shipping.clone())
                    .unwrap_or_else(|| "—".into()),
                super::money_of(o.total.as_ref()),
            ]);
        }
        writeln!(out, "{t}")?;
        writeln!(
            out,
            "Page {} of {}. One order: `bgnz orders show <number>`.",
            p.page, p.pages
        )
    }
}

#[derive(Serialize)]
pub struct OrderView<'a> {
    pub order: &'a OrderDetail,
}

impl View for OrderView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let o = self.order;
        writeln!(
            out,
            "{}  {}  {}",
            out.heading(&o.number),
            o.date.as_deref().unwrap_or(""),
            o.status.as_deref().unwrap_or("")
        )?;

        for s in &o.shipments {
            writeln!(out)?;
            writeln!(
                out,
                "{} — {}",
                s.name.as_deref().unwrap_or("Shipment"),
                s.status.as_deref().unwrap_or("—")
            )?;
            if let Some(tracking) = &s.tracking {
                writeln!(out, "{tracking}")?;
            }
            let mut t = table(&["Product", "Qty", "Price", "Total"]);
            for line in &s.lines {
                t.add_row(vec![
                    line.name.clone(),
                    super::quantity(line.quantity),
                    super::money_of(line.price.as_ref()),
                    super::money_of(line.total.as_ref()),
                ]);
            }
            writeln!(out, "{t}")?;
        }

        writeln!(out)?;
        // The same running-total shape as every other tool's cart and order:
        // subtotal, then each thing that moved it, then the total in bold.
        if let Some(subtotal) = &o.subtotal {
            writeln!(out, "Subtotal {}", super::money(Some(subtotal.value)))?;
        }
        if let Some(shipping) = o.shipping.as_ref().filter(|s| s.value > 0.0) {
            writeln!(out, "Delivery {}", super::money(Some(shipping.value)))?;
        }
        for d in &o.discounts {
            writeln!(
                out,
                "{} -{}",
                d.label.as_deref().unwrap_or("Discount"),
                super::money(Some(d.amount.value))
            )?;
        }
        if let Some(reward) = &o.loyalty_reward {
            writeln!(out, "Loyalty reward -{}", super::money(Some(reward.value)))?;
        }
        if let Some(total) = &o.total {
            writeln!(
                out,
                "{}",
                out.heading(&format!("Total {}", super::money(Some(total.value))))
            )?;
        }
        if !o.invoices.is_empty() {
            writeln!(
                out,
                "{}",
                out.dim(&format!("invoice: `bgnz orders invoice {}`", o.number))
            )?;
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct ReceiptList<'a> {
    pub page: &'a ReceiptPage,
    pub banner: String,
}

impl<'a> ReceiptList<'a> {
    pub fn new(page: &'a ReceiptPage, banner: bgnz_api::Banner) -> ReceiptList<'a> {
        ReceiptList {
            page,
            banner: banner.name().to_string(),
        }
    }
}

impl View for ReceiptList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let p = self.page;
        if p.receipts.is_empty() {
            return writeln!(
                out,
                "No {} shop receipts in this window. They are matched to the loyalty \
                 membership, so a purchase made without scanning will not appear.",
                self.banner
            );
        }
        for r in &p.receipts {
            writeln!(
                out,
                "{}  {}  {}",
                out.heading(r.store.as_deref().unwrap_or("In store")),
                r.date.as_deref().unwrap_or(""),
                super::money(r.total)
            )?;
            let mut t = table(&["Code", "Product", "Qty", "Total"]);
            for line in &r.lines {
                t.add_row(vec![
                    line.code.clone().unwrap_or_else(|| "—".into()),
                    line.name.clone(),
                    super::quantity(line.quantity),
                    super::money(line.total),
                ]);
            }
            writeln!(out, "{t}")?;
            if let Some(reference) = &r.reference {
                writeln!(out, "{}", out.dim(&format!("receipt {reference}")))?;
            }
            writeln!(out)?;
        }
        // The service picks the window when none was asked for, and only says
        // so on the records themselves -- so saying it back is the only way a
        // person knows what was searched.
        match (&p.from, &p.to) {
            (Some(from), Some(to)) => writeln!(
                out,
                "{} receipt{} between {from} and {to}.{}",
                p.receipts.len(),
                cli_kit::plural(p.receipts.len()),
                if p.more_history {
                    " There is older history: pass --from to reach it."
                } else {
                    ""
                }
            ),
            _ => super::write_count(out, p.receipts.len(), "receipt", None),
        }
    }
}
