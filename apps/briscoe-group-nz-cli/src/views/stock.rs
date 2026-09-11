//! Click-and-collect answers.
//!
//! Two views because the service answers two different questions. Per product
//! it says whether that one thing can be collected; per basket it gives **one**
//! verdict for everything asked about, and the worst line decides it. Rendering
//! the second as a per-line table would invent detail the service never sent.

use std::io::{self, Write};

use bgnz_api::Stock;
use cli_kit::{table, Out, View};
use serde::Serialize;

/// One row per thing that could be asked about -- which for a configurable
/// product is one row per variant, because only a variant has a barcode.
#[derive(Serialize)]
pub struct StockList {
    pub sku: String,
    pub store: String,
    pub store_id: i64,
    pub rows: Vec<StockRow>,
}

#[derive(Serialize)]
pub struct StockRow {
    pub sku: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl View for StockList {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        writeln!(out, "{}", out.heading(&self.store))?;
        if self.rows.is_empty() {
            return writeln!(
                out,
                "Nothing in {} carries a barcode, so the collection service cannot \
                 identify it.",
                self.sku
            );
        }
        let mut t = table(&["SKU", "Variant", "Status", "Collection"]);
        for row in &self.rows {
            t.add_row(vec![
                row.sku.clone(),
                row.variant.clone().unwrap_or_else(|| "—".into()),
                row.pickup_status.clone().unwrap_or_else(|| "—".into()),
                row.message.clone().unwrap_or_else(|| "—".into()),
            ]);
        }
        writeln!(out, "{t}")?;
        super::write_count(out, self.rows.len(), "variant", None)
    }
}

/// The one verdict for a whole basket.
#[derive(Serialize)]
pub struct BasketStock<'a> {
    pub stock: &'a Stock,
    pub store: String,
}

impl View for BasketStock<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let s = self.stock;
        let status = s.pickup_status.as_deref().unwrap_or("unknown");
        let label = match status {
            "IN_STOCK" | "IN_STOCK_AFTER_CUTOFF" => out.good(status),
            "OOS" | "OUT_OF_STOCK" => out.bad(status),
            other => out.warn(other),
        };
        writeln!(out, "{}", out.heading(&self.store))?;
        writeln!(out, "{label}  {}", s.message.as_deref().unwrap_or(""))?;
        writeln!(
            out,
            "{}",
            out.dim(&format!(
                "one verdict for all {} item{} — the service answers per basket, not per line",
                s.skus.len(),
                cli_kit::plural(s.skus.len())
            ))
        )
    }
}
