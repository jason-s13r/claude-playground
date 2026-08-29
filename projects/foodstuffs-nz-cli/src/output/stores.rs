//! Rendering a store list.

use crate::domain::Store;
use crate::output::{plural, table};

pub fn print_stores(stores: &[Store]) {
    let mut t = table();
    t.set_header(vec!["ID", "Name", "Region"]);
    for s in stores {
        t.add_row(vec![
            s.id.clone(),
            s.name.clone(),
            s.region.clone().unwrap_or_default(),
        ]);
    }
    println!("{t}");
    println!(
        "{} store{}. Select one: fsnz store set <id or name fragment>",
        stores.len(),
        plural(stores.len())
    );
}
