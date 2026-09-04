//! `departments` -- the category tree.

use cli_kit::emit;

use crate::app::App;
use crate::error::AppResult;
use crate::views::DepartmentTree;

/// The top of the tree.
///
/// Hard-coded because there is no call that answers "what are the roots":
/// `Category-GetMultipleNavigationHierarchy` takes the ids it should describe.
/// Each of these was verified against the live site -- the id is *not* the
/// `/c/` path with its hyphens removed, which is the obvious guess and is wrong
/// for four of them (`craft-party-stationery` is `stationerycraftparty`,
/// `books` is `booksmusicmovies`, `travel` is `luggagetravel`).
///
/// A root that goes stale costs that one department: the endpoint answers with
/// a refusal in its place and the parse drops it.
const ROOTS: [&str; 14] = [
    "homegarden",
    "clothingshoesaccessories",
    "toysbaby",
    "electronicsgaming",
    "sportsoutdoors",
    "foodhouseholdpets",
    "healthbeauty",
    "stationerycraftparty",
    "booksmusicmovies",
    "autodiy",
    "luggagetravel",
    "gifting",
    "officialmerchandise",
    "specials",
];

pub async fn run(app: &App, query: Option<String>, depth: u32) -> AppResult<()> {
    let client = app.client()?;
    let departments = client.categories(&ROOTS, depth).await?;

    // Filtering here rather than in the request: the endpoint takes ids, so
    // narrowing by a name someone typed means having the tree first.
    let shown = match &query {
        Some(needle) => departments
            .iter()
            .filter_map(|d| d.find(needle).cloned())
            .collect(),
        None => departments,
    };

    emit(
        &mut app.out(),
        &DepartmentTree {
            departments: &shown,
        },
    )?;
    Ok(())
}
