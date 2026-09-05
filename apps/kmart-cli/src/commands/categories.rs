//! `categories` -- the tree, or a subtree of it.

use cli_kit::emit;

use crate::app::App;
use crate::error::AppResult;
use crate::views::CategoryTree;

pub async fn run(app: &App, query: Option<&str>, depth: u32) -> AppResult<()> {
    let client = app.client().await?;
    // One request whatever the depth: the whole tree rides along on a listing,
    // so asking for more levels costs nothing extra.
    let tree = client.categories(depth.max(1)).await?;

    let categories = match query {
        None => tree,
        Some(query) => {
            let wanted = query.trim().to_lowercase();
            tree.iter()
                .flat_map(|c| c.walk())
                .filter(|c| c.name.to_lowercase().contains(&wanted))
                .cloned()
                .collect()
        }
    };

    let mut out = app.out();
    Ok(emit(
        &mut out,
        &CategoryTree {
            categories: &categories,
            depth: depth.max(1),
        },
    )?)
}
