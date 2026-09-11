//! `categories` -- the whole tree, filtered down to something readable.
//!
//! One request: the tree is 961 categories for Briscoes and 1,004 for Rebel
//! Sport, which is small enough to fetch whole and narrow here rather than
//! walking it a level at a time and spending a request per level.

use cli_kit::emit;

use crate::app::App;
use crate::error::AppResult;
use crate::views::CategoryTree;

pub async fn run(app: &App, query: Option<String>, depth: i64) -> AppResult<()> {
    let mut categories = app.client()?.categories().await?;
    let needle = query.map(|q| q.to_lowercase());

    categories.retain(|c| match &needle {
        // A search means "show me this wherever it is", so the depth limit
        // would only hide what was asked for.
        Some(needle) => {
            c.name.to_lowercase().contains(needle)
                || c.url_path
                    .as_deref()
                    .is_some_and(|p| p.to_lowercase().contains(needle))
        }
        None => c.level <= depth,
    });
    categories.sort_by(|a, b| {
        a.url_path
            .as_deref()
            .unwrap_or(&a.name)
            .cmp(b.url_path.as_deref().unwrap_or(&b.name))
    });

    let mut out = app.out();
    emit(&mut out, &CategoryTree::new(&categories, app.banner))?;
    Ok(())
}
