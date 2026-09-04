//! Finding products: search, browse, and the paging both share.
//!
//! One endpoint answers both. `/search/updategrid` takes either `q=` for a
//! keyword or `cgid=` for a category and returns the same HTML fragment of
//! tiles either way, which is why there is one [`Query`] type rather than two.
//!
//! Paging is a **window**, not a cumulative refetch: `sz` stays constant while
//! `start` walks, and each response holds exactly that many tiles. Random
//! access works -- `start=288` can be asked for without walking the pages
//! before it -- so a listing can be resumed, and a page N costs one request.

use crate::domain::Island;

/// The window size the site's own grid asks for. Nothing documents a ceiling
/// and a larger one has not been tried, so this stays at what is known to work
/// and pages for the rest.
pub const PAGE_SIZE: u32 = 32;

/// The site's own default ordering for a browse.
pub const DEFAULT_SORT: &str = "default-navigation";

/// The sort rules the site's own menu offers.
///
/// A listing page publishes its own list -- [`crate::extract::sort_options`]
/// reads it -- and an unfamiliar value is passed through rather than rejected
/// here. This exists so a `--sort` help string has something to suggest without
/// a round trip.
pub const SORTS: [&str; 8] = [
    "default-navigation",
    "price-low-to-high",
    "price-high-to-low",
    "best-sellers",
    "new-arrivals",
    "top-rated",
    "product-name-ascending",
    "product-name-descending",
];

/// How the ordering is named on the wire.
pub type Sort = String;

/// One refinement. SFCC numbers these in pairs -- `prefn1`/`prefv1`,
/// `prefn2`/`prefv2` -- so the position matters and a `Vec` of these is what
/// builds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facet {
    pub name: String,
    pub value: String,
}

impl Facet {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Facet {
        Facet {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// The facet names observed on this storefront, for a `--help` that can say
/// what is filterable. Not exhaustive and not enforced -- an unknown name is
/// sent as given.
pub const FACETS: [&str; 8] = [
    "brand",
    "color",
    "size",
    "clearance",
    "channelType",
    "marketplaceItem",
    "season",
    "bvAverageRating",
];

/// What to list.
#[derive(Clone, Debug)]
pub enum Query {
    /// A keyword. May redirect into a category -- see [`Listing::category`].
    Keyword(String),
    /// A category id, as `cgid`. Not the `/c/...` path.
    Category(String),
}

impl Query {
    pub fn param(&self) -> (&'static str, &str) {
        match self {
            Query::Keyword(q) => ("q", q),
            Query::Category(c) => ("cgid", c),
        }
    }

    pub fn describes(&self) -> &str {
        match self {
            Query::Keyword(q) | Query::Category(q) => q,
        }
    }
}

/// One window of results.
pub struct Listing {
    pub products: Vec<crate::Product>,
    /// How many the whole listing holds, when the page said. Only the header of
    /// a listing carries this, so it can be absent on a fragment that has been
    /// trimmed.
    pub total: Option<u32>,
    /// The category a keyword search landed in, when it redirected into one.
    ///
    /// Worth surfacing rather than hiding: paging afterwards has to switch from
    /// `q=` to `cgid=`, and the caller is the only one that can decide whether
    /// it wanted a search or a department.
    pub category: Option<String>,
}

/// The query string for one window.
///
/// Built as pairs rather than with a URL type because the facet parameters are
/// positional -- `prefn1` has to line up with `prefv1` -- and because the sort
/// rule and the island refinement have to be in every request, not just the
/// first.
pub fn params(
    query: &Query,
    start: u32,
    size: u32,
    sort: Option<&str>,
    island: Option<Island>,
    facets: &[Facet],
) -> Vec<(String, String)> {
    let (key, value) = query.param();
    let mut out = vec![
        (key.to_string(), value.to_string()),
        ("start".to_string(), start.to_string()),
        ("sz".to_string(), size.to_string()),
    ];
    if let Some(sort) = sort {
        out.push(("srule".to_string(), sort.to_string()));
    }

    // The island is a refinement like any other, so it occupies a numbered
    // slot and has to be counted alongside the caller's own.
    let mut n = 1;
    if let Some(island) = island {
        out.push((format!("prefn{n}"), "islandAvailability".to_string()));
        out.push((format!("prefv{n}"), island.value().to_string()));
        n += 1;
    }
    for facet in facets {
        out.push((format!("prefn{n}"), facet.name.clone()));
        out.push((format!("prefv{n}"), facet.value.clone()));
        n += 1;
    }
    out
}

/// The category a `/c/...` URL names, for detecting a search that redirected.
pub fn category_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next()?;
    let rest = path.split("/c/").nth(1)?.trim_end_matches('/');
    (!rest.is_empty()).then(|| rest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_names_its_own_start_and_size() {
        let p = params(&Query::Keyword("blue".into()), 64, 32, None, None, &[]);
        assert!(p.contains(&("q".to_string(), "blue".to_string())));
        assert!(p.contains(&("start".to_string(), "64".to_string())));
        assert!(p.contains(&("sz".to_string(), "32".to_string())));
    }

    #[test]
    fn a_browse_and_a_search_differ_only_in_one_parameter() {
        let s = params(&Query::Keyword("x".into()), 0, 32, None, None, &[]);
        let b = params(&Query::Category("toysbaby".into()), 0, 32, None, None, &[]);
        assert_eq!(s[0].0, "q");
        assert_eq!(b[0].0, "cgid");
        assert_eq!(s[1..], b[1..]);
    }

    #[test]
    fn the_island_takes_a_refinement_slot_and_pushes_the_rest_along() {
        // The bug this pins: numbering the caller's facets from 1 while the
        // island also claims 1 silently drops one of them.
        let p = params(
            &Query::Category("toysbaby".into()),
            0,
            32,
            Some("price-low-to-high"),
            Some(Island::North),
            &[Facet::new("brand", "LEGO"), Facet::new("color", "Blue")],
        );
        let pairs: std::collections::HashMap<_, _> = p.into_iter().collect();
        assert_eq!(pairs["srule"], "price-low-to-high");
        assert_eq!(pairs["prefn1"], "islandAvailability");
        assert_eq!(pairs["prefv1"], "northIsland");
        assert_eq!(pairs["prefn2"], "brand");
        assert_eq!(pairs["prefv2"], "LEGO");
        assert_eq!(pairs["prefn3"], "color");
        assert_eq!(pairs["prefv3"], "Blue");
    }

    #[test]
    fn facets_start_at_one_when_no_island_is_set() {
        let p = params(
            &Query::Keyword("x".into()),
            0,
            32,
            None,
            None,
            &[Facet::new("brand", "LEGO")],
        );
        let pairs: std::collections::HashMap<_, _> = p.into_iter().collect();
        assert_eq!(pairs["prefn1"], "brand");
    }

    #[test]
    fn a_search_that_lands_on_a_category_is_recognised_by_its_url() {
        // Observed: `q=lego` 302s to a brand category, after which paging has
        // to use `cgid` -- so the redirect has to be noticed, not followed
        // silently.
        assert_eq!(
            category_from_url("https://www.thewarehouse.co.nz/c/toys-baby/top-brands/lego?sr=lego")
                .as_deref(),
            Some("toys-baby/top-brands/lego")
        );
        assert_eq!(
            category_from_url("https://www.thewarehouse.co.nz/search?q=blue"),
            None
        );
    }
}
