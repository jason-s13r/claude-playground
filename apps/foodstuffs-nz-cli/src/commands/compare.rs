//! `fsnz compare` -- the same search run at both banners, side by side.

use anyhow::Result;

use crate::api;
use crate::app::App;
use crate::banner::Banner;
use crate::cli::ListArgs;
use crate::commands::io::print_json;
use crate::commands::products::apply_size_filter;
use crate::domain::compare::pair;
use crate::domain::Product;
use crate::output;

pub async fn run(app: &App, query: &str, specials_only: bool, args: &ListArgs) -> Result<()> {
    let banners = Banner::ALL;

    // Both stores have to be resolvable before either request goes out, so a
    // half-finished comparison never gets printed.
    let store_ids: Vec<String> = banners
        .iter()
        .map(|b| app.store_id(*b))
        .collect::<Result<_>>()?;

    let mut sides: Vec<Vec<Product>> = Vec::new();
    for (banner, store_id) in banners.iter().zip(&store_ids) {
        let (client, _) = app.client(*banner, false, true).await?;
        let filters = api::filters(store_id, specials_only, None);
        let result = client
            .collect(store_id, query, &filters, args.limit, &args.sort)
            .await?;
        sides.push(apply_size_filter(result.products, args.size.as_deref()));
    }

    let rows = pair(&sides);

    if app.json {
        let json_rows: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                let prices: serde_json::Map<String, serde_json::Value> = banners
                    .iter()
                    .zip(&row.sides)
                    .map(|(b, side)| {
                        (
                            b.id().to_string(),
                            match side {
                                Some(p) => {
                                    serde_json::to_value(p).unwrap_or(serde_json::Value::Null)
                                }
                                None => serde_json::Value::Null,
                            },
                        )
                    })
                    .collect();
                serde_json::json!({
                    "title": row.title,
                    "size": row.size,
                    "found_at_both": row.matched(),
                    "difference": row.saving().map(|c| c as f64 / 100.0),
                    "cheapest": row.cheapest().map(|i| banners[i].id()),
                    "banners": prices,
                })
            })
            .collect();
        let stores_json: serde_json::Map<String, serde_json::Value> = banners
            .iter()
            .zip(&store_ids)
            .map(|(b, s)| (b.id().to_string(), serde_json::Value::String(s.clone())))
            .collect();
        print_json(&serde_json::json!({
            "query": query,
            "banners": banners.iter().map(|b| b.id()).collect::<Vec<_>>(),
            "stores": stores_json,
            "count": json_rows.len(),
            "rows": json_rows,
        }));
        return Ok(());
    }

    if rows.is_empty() {
        println!("No products found for '{query}' at either banner.");
        return Ok(());
    }

    output::print_comparison(&banners, &rows);
    Ok(())
}
