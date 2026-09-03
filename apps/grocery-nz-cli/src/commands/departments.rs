//! `departments` -- the tree, or the part of it under one node.

use cli_kit::emit;
use gsnz_core::department;
use gsnz_ui::DepartmentTree;

use crate::app::App;
use crate::error::AppResult;

pub async fn run(app: &App, query: Option<String>, depth: u32) -> AppResult<()> {
    let handle = app.handle()?;
    let all = handle.departments().await?;
    let shown = match query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        Some(needle) => vec![department::find(&all, needle)
            .ok_or_else(|| {
                crate::error::AppError::usage(format!(
                    "no department matching {needle:?}: run `gsnz departments` for the list"
                ))
            })?
            .clone()],
        None => all,
    };
    emit(&mut app.out(), &DepartmentTree::new(&shown, depth))?;
    Ok(())
}
