//! Rendering the two banners side by side.

use comfy_table::Cell;

use crate::banner::Banner;
use crate::domain::compare::Row;
use crate::domain::dollars;
use crate::output::products::price_label;
use crate::output::{plural, table};

pub fn print_comparison(banners: &[Banner], rows: &[Row]) {
    let mut t = table();
    let mut header: Vec<Cell> = vec![Cell::new("Product"), Cell::new("Size")];
    header.extend(banners.iter().map(|b| Cell::new(b.name())));
    header.push(Cell::new("Difference"));
    t.set_header(header);

    for row in rows {
        let cheapest = row.cheapest();
        let mut cells: Vec<Cell> = vec![
            Cell::new(&row.title),
            Cell::new(row.size.clone().unwrap_or_default()),
        ];
        for (i, side) in row.sides.iter().enumerate() {
            let text = match side {
                Some(p) => {
                    let base = price_label(p);
                    if cheapest == Some(i) {
                        format!("{base}  ←")
                    } else {
                        base
                    }
                }
                // Absence here means "not in this banner's results", which is
                // not the same as "unavailable" -- each banner is searched
                // independently and the limit may simply have cut it off.
                None => "—".to_string(),
            };
            cells.push(Cell::new(text));
        }
        cells.push(Cell::new(match row.saving() {
            Some(0) => "same".to_string(),
            Some(c) => dollars(c),
            None => String::new(),
        }));
        t.add_row(cells);
    }
    println!("{t}");

    let matched = rows.iter().filter(|r| r.matched()).count();
    println!(
        "{} product{} compared, {} found at both. \
         ← cheaper banner. — not in that banner's results, which is not the same \
         as unavailable.",
        rows.len(),
        plural(rows.len()),
        matched,
    );
}
