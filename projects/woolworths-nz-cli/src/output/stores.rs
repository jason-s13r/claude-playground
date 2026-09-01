//! Rendering a store list.

use crate::domain::Store;
use crate::output::{plural, table};

pub fn print_stores(stores: &[Store]) {
    let mut t = table();
    let has_distance = stores.iter().any(|s| s.distance_km.is_some());
    let mut header = vec!["ID", "Name", "Where"];
    if has_distance {
        header.push("Distance");
    }
    t.set_header(header);

    for s in stores {
        let mut row = vec![s.id.clone(), s.name.clone(), s.where_it_is()];
        if has_distance {
            row.push(
                s.distance_km
                    .map(|d| format!("{d:.1} km"))
                    .unwrap_or_default(),
            );
        }
        t.add_row(row);
    }
    println!("{t}");
    println!(
        "{} store{}. Select one: wwnz store set <id or name fragment>",
        stores.len(),
        plural(stores.len())
    );
}
