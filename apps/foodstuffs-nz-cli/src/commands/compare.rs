//! `compare` -- the reason this binary exists.
//!
//! The join itself is [`gsnz_core::pair`], because it is pure logic over
//! products, shared with the combined tool. What is here is the fan-out, the
//! flags, and reporting the banners that could not answer.

use cli_kit::emit;
use gsnz_core::{Error, RetailerId, Search, SearchBy};
use gsnz_ui::CompareTable;

use crate::app::App;
use crate::cli::Listing;
use crate::config::MatchMode;
use crate::error::{AppError, AppResult};
use crate::retailers::Handle;

pub async fn run(app: &App, query: String, flags: Listing, strict: bool) -> AppResult<()> {
    let span = app.compare_span();
    if span.len() < 2 {
        return Err(AppError::usage(
            "comparing needs both banners: drop `-b`, or name them as `-b nw,pns`",
        ));
    }
    let (handles, mut failures) = app.handles(&span);

    let search = Search {
        by: SearchBy::Query(query),
        specials_only: flags.specials,
        limit: flags.limit,
        size: flags.size,
        sort: flags.sort,
    };
    let (sides, more) = fan_out(&handles, &search).await;
    failures.extend(more);

    // A banner that answered nothing still gets a column: an empty one says
    // "not stocked here", which a missing column does not.
    let shown: Vec<RetailerId> = handles.iter().map(|h| h.id()).collect();
    let allow_normalised = !strict && app.config.compare.r#match == MatchMode::Normalised;
    let rows = gsnz_core::pair(&sides, allow_normalised);

    emit(
        &mut app.out(),
        &CompareTable {
            retailers: &shown,
            rows: &rows,
        },
    )?;

    // On stderr, so a shop being down does not corrupt `--json` output that a
    // script is reading -- and so it is still seen when one is not.
    for (id, e) in &failures {
        eprintln!("fsnz: {id} could not be included: {e}");
    }
    Ok(())
}

/// Both banners at once. One failure does not sink the rest: a lapsed session
/// at one banner must not hide the other's price.
async fn fan_out(
    handles: &[Handle],
    search: &Search,
) -> (Vec<Vec<gsnz_core::Product>>, Vec<(RetailerId, Error)>) {
    let results = futures_join(handles, search).await;
    let mut sides = Vec::with_capacity(results.len());
    let mut failures = Vec::new();
    for (id, result) in results {
        match result {
            Ok(found) => sides.push(found.products),
            Err(e) => {
                sides.push(Vec::new());
                failures.push((id, e));
            }
        }
    }
    (sides, failures)
}

/// Concurrent without a `futures` dependency: one task per banner, and
/// `tokio::join!` needs a fixed arity this does not have.
async fn futures_join(
    handles: &[Handle],
    search: &Search,
) -> Vec<(RetailerId, gsnz_core::Result<gsnz_core::SearchResult>)> {
    let mut tasks = Vec::with_capacity(handles.len());
    for handle in handles {
        let handle = handle.clone();
        let search = search.clone();
        tasks.push(tokio::spawn(async move {
            (handle.id(), handle.search(&search).await)
        }));
    }
    let mut out = Vec::with_capacity(tasks.len());
    for task in tasks {
        match task.await {
            Ok(result) => out.push(result),
            // A panicked task is a bug here, not upstream, and saying so beats
            // reporting it as the shop being unavailable.
            Err(e) => out.push((
                RetailerId::NewWorld,
                Err(Error::Other(format!("a search task failed: {e}"))),
            )),
        }
    }
    out
}
