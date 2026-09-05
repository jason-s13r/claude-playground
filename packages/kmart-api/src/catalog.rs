//! The catalogue: Constructor.io.
//!
//! The half of Kmart that answers a cold client, because it is not Kmart --
//! it is a third-party search index serving Kmart's product feed, and it
//! authorises on a public key that the storefront ships in its own JavaScript.
//! Search, browse, the category tree, the facets and one product's full record
//! all come from here, which is why this crate can do the reading half without
//! a browser anywhere in the picture.
//!
//! One request shape serves all of it. `/search/<term>` and
//! `/browse/group_id/<id>` take the same parameters and answer the same
//! envelope, and a **keycode is a term the index matches exactly** -- so
//! fetching one product is a search, not a lookup, and there is no item
//! endpoint to keep working.

use crate::country::Country;
use crate::domain::{
    Category, Facet, FacetOption, Listing, Price, Product, Rating, Sort, Variation,
};
use crate::endpoints::{query_string, Endpoints, SEARCH_CLIENT};
use crate::wire;

/// What the storefront asks for per page.
pub const PAGE_SIZE: u32 = 60;

/// The sorts the index offers, as it spells them. `relevance` descending is
/// what the site calls "Popular" and is what a listing with no sort gets.
pub const SORTS: [(&str, &str, &str); 4] = [
    ("relevance", "descending", "popular"),
    ("numberOfDaysSinceStartDate", "ascending", "new"),
    ("price", "ascending", "price-low-to-high"),
    ("price", "descending", "price-high-to-low"),
];

/// The default, matching the storefront's.
pub const DEFAULT_SORT: (&str, &str) = ("relevance", "descending");

/// Turn a friendly sort name into what the index wants.
///
/// An unfamiliar value is passed straight through as `sort_by` rather than
/// refused: the index publishes its own sort list per listing, and refusing
/// one this table has not heard of would age badly.
pub fn sort_for(name: &str) -> (String, String) {
    let name = name.trim();
    for (by, order, alias) in SORTS {
        if name.eq_ignore_ascii_case(alias) || name.eq_ignore_ascii_case(by) {
            return (by.to_string(), order.to_string());
        }
    }
    (name.to_string(), "descending".to_string())
}

/// Where the products come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Select {
    /// A keyword, or a keycode -- the index does not distinguish.
    Term(String),
    /// A category, by its 32-hex group id.
    Group(String),
}

/// One listing request.
#[derive(Clone, Debug)]
pub struct Query {
    pub select: Select,
    pub page: u32,
    pub per_page: u32,
    pub sort: Option<(String, String)>,
    /// Refinements, as `(facet name, value)`. A `Vec` rather than a map
    /// because one facet takes more than one value and the index reads the
    /// repeats.
    pub filters: Vec<(String, String)>,
}

impl Query {
    pub fn term(term: impl Into<String>) -> Query {
        Query::new(Select::Term(term.into()))
    }

    pub fn group(id: impl Into<String>) -> Query {
        Query::new(Select::Group(id.into()))
    }

    fn new(select: Select) -> Query {
        Query {
            select,
            page: 1,
            per_page: PAGE_SIZE,
            sort: None,
            filters: Vec::new(),
        }
    }

    pub fn page(mut self, page: u32) -> Query {
        self.page = page.max(1);
        self
    }

    pub fn per_page(mut self, n: u32) -> Query {
        self.per_page = n.clamp(1, 200);
        self
    }

    pub fn sort(mut self, name: &str) -> Query {
        self.sort = Some(sort_for(name));
        self
    }

    pub fn filter(mut self, name: impl Into<String>, value: impl Into<String>) -> Query {
        self.filters.push((name.into(), value.into()));
        self
    }

    /// Keep only what is ranged in one island.
    ///
    /// New Zealand only, and it is a real filter rather than a display
    /// preference: Kmart ranges differently across the strait, so this changes
    /// what the listing *contains*. Australia has no equivalent facet, so
    /// asking for one there is a no-op rather than an error -- the caller
    /// should not have to branch on the country to build a query.
    pub fn island(self, country: Country, island: Option<&str>) -> Query {
        match (country, island) {
            (Country::Nz, Some(code)) => self.filter(format!("Available in {code}"), "True"),
            _ => self,
        }
    }

    /// The URL, key and client identity included.
    ///
    /// `client_id` is the visitor id the index personalises on. Taken as an
    /// argument rather than generated here so that it is stable across runs --
    /// a new one each time is a new visitor each time, which is both worse
    /// ranking and a louder pattern than a returning one.
    pub fn url(&self, endpoints: &Endpoints, client_id: &str) -> String {
        let base = match &self.select {
            Select::Term(term) => endpoints.search_term(term),
            Select::Group(id) => endpoints.browse_group(id),
        };
        let mut params: Vec<(String, String)> = vec![
            ("key".into(), endpoints.search_key.clone()),
            ("i".into(), client_id.to_string()),
            ("s".into(), "1".into()),
            ("c".into(), SEARCH_CLIENT.into()),
            ("page".into(), self.page.to_string()),
            ("num_results_per_page".into(), self.per_page.to_string()),
        ];
        for (name, value) in &self.filters {
            params.push((format!("filters[{name}]"), value.clone()));
        }
        let (by, order) = self
            .sort
            .clone()
            .unwrap_or_else(|| (DEFAULT_SORT.0.into(), DEFAULT_SORT.1.into()));
        params.push(("sort_by".into(), by));
        params.push(("sort_order".into(), order));
        format!("{base}?{}", query_string(&params))
    }
}

/// The tree request: every category, to a depth.
///
/// `group_id/all` is the index's root. The products it returns are incidental
/// -- one is asked for and thrown away -- because the groups ride along on a
/// product listing rather than having an endpoint of their own.
pub fn tree_url(endpoints: &Endpoints, client_id: &str, depth: u32) -> String {
    let params: Vec<(String, String)> = vec![
        ("key".into(), endpoints.search_key.clone()),
        ("i".into(), client_id.to_string()),
        ("s".into(), "1".into()),
        ("c".into(), SEARCH_CLIENT.into()),
        ("page".into(), "1".into()),
        ("num_results_per_page".into(), "1".into()),
        (
            "fmt_options[groups_max_depth]".into(),
            depth.clamp(1, 6).to_string(),
        ),
    ];
    format!(
        "{}?{}",
        endpoints.browse_group("all"),
        query_string(&params)
    )
}

// ------------------------------------------------------------------ decoding

/// One product out of an index record.
fn product(result: &wire::SearchResult) -> Option<Product> {
    let d = &result.data;
    let keycode = d.variation_id.clone().or_else(|| {
        // `id` is the keycode with a `P_` prefix, and is the fallback for a
        // record that somehow arrived without the plain one.
        d.id.as_deref()
            .and_then(|id| id.strip_prefix("P_"))
            .map(str::to_string)
    })?;

    // The `price` number is the effective one -- what the site renders. The
    // `prices` array is where a currency is stated, and where a higher `list`
    // entry would reveal a markdown.
    let price = d.price.map(Price::from_dollars).or_else(|| {
        d.prices
            .first()
            .and_then(|p| p.amount.as_deref())
            .and_then(Price::parse)
    });
    let was = d
        .prices
        .iter()
        .filter(|p| p.kind.as_deref() == Some("list"))
        .filter_map(|p| p.amount.as_deref().and_then(Price::parse))
        .filter(|listed| price.is_some_and(|now| listed.cents > now.cents))
        .max();

    Some(Product {
        keycode,
        name: result.value.clone().unwrap_or_default(),
        brand: d.brand.clone(),
        price,
        was,
        currency: d.prices.first().and_then(|p| p.currency.clone()),
        colour: d.colour.clone(),
        size: d.size.clone(),
        seller: d.seller.first().cloned(),
        rating: d.ratings.as_ref().and_then(|r| {
            Some(Rating {
                score: r.average_score?,
                reviews: r.total_reviews.unwrap_or(0),
            })
        }),
        url: d.url.clone(),
        image: d.image_url.clone(),
        images: d.alt_images.clone(),
        description: d.description.clone(),
        free_shipping: d.free_shipping.unwrap_or(false),
        national_inventory: d.national_inventory.unwrap_or(false),
        category_id: d.primary_category_id.clone(),
        variations: result
            .variations
            .iter()
            .filter_map(|v| {
                Some(Variation {
                    keycode: v.data.variation_id.clone()?,
                    name: v.value.clone(),
                    colour: v.data.colour.clone(),
                    size: v.data.size.clone(),
                    price: v.data.price.map(Price::from_dollars),
                })
            })
            .collect(),
        extra: d.extra.clone(),
    })
}

/// A whole listing response.
pub fn listing(body: &str) -> crate::Result<Listing> {
    let env: wire::SearchEnvelope =
        serde_json::from_str(body).map_err(|e| crate::Error::decode("reading a listing", e))?;
    let r = env.response;
    Ok(Listing {
        products: r.results.iter().filter_map(product).collect(),
        total: r.total_num_results,
        exact: r.result_sources.get("token_match").map(|s| s.count),
        facets: r
            .facets
            .iter()
            .filter_map(|f| {
                Some(Facet {
                    name: f.name.clone()?,
                    label: f.display_name.clone(),
                    options: f
                        .options
                        .iter()
                        .filter_map(|o| {
                            Some(FacetOption {
                                value: o.value.clone()?,
                                label: o.display_name.clone(),
                                count: o.count,
                            })
                        })
                        .collect(),
                })
            })
            .collect(),
        sorts: r
            .sort_options
            .iter()
            .filter_map(|s| {
                Some(Sort {
                    by: s.sort_by.clone()?,
                    order: s.sort_order.clone().unwrap_or_else(|| "descending".into()),
                    label: s.display_name.clone(),
                })
            })
            .collect(),
    })
}

/// The categories out of a tree response.
pub fn categories(body: &str) -> crate::Result<Vec<Category>> {
    let env: wire::SearchEnvelope = serde_json::from_str(body)
        .map_err(|e| crate::Error::decode("reading the category tree", e))?;
    // The root group is `all`, which is a container rather than a category, so
    // what a caller wants is its children.
    Ok(env
        .response
        .groups
        .iter()
        .flat_map(|root| {
            if root.group_id.as_deref() == Some("all") {
                root.children
                    .iter()
                    .filter_map(category)
                    .collect::<Vec<_>>()
            } else {
                category(root).into_iter().collect()
            }
        })
        .collect())
}

fn category(g: &wire::Group) -> Option<Category> {
    Some(Category {
        id: g.group_id.clone()?,
        name: g.display_name.clone().unwrap_or_default(),
        path: g.data.url.clone(),
        count: g.count,
        children: g.children.iter().filter_map(category).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap()
    }

    fn endpoints() -> Endpoints {
        Endpoints::of(Country::Nz)
    }

    #[test]
    fn a_search_url_carries_the_countrys_key_and_the_default_sort() {
        let url = Query::term("milk frother").url(&endpoints(), "visitor-1");
        assert!(url.starts_with("https://ac.cnstrc.com/search/milk%20frother?"));
        assert!(
            url.contains(&format!("key={}", Country::Nz.search_key())),
            "{url}"
        );
        assert!(url.contains("i=visitor-1"));
        assert!(
            url.contains("sort_by=relevance&sort_order=descending"),
            "{url}"
        );
    }

    #[test]
    fn the_australian_key_is_the_one_used_for_australia() {
        // The failure this catches is silent: the wrong key answers 200 with
        // the other country's products at the other country's prices.
        let url = Query::term("mop").url(&Endpoints::of(Country::Au), "v");
        assert!(
            url.contains(&format!("key={}", Country::Au.search_key())),
            "{url}"
        );
        assert_ne!(Country::Au.search_key(), Country::Nz.search_key());
    }

    #[test]
    fn a_keycode_is_just_a_term() {
        let url = Query::term("43165537").per_page(1).url(&endpoints(), "v");
        assert!(url.contains("/search/43165537?"), "{url}");
        assert!(url.contains("num_results_per_page=1"));
    }

    #[test]
    fn repeated_filters_survive_into_the_query_string() {
        let url = Query::group("abc")
            .filter("Colour", "Black")
            .filter("Colour", "White")
            .url(&endpoints(), "v");
        assert_eq!(
            url.matches("filters%5BColour%5D=").count(),
            2,
            "a map would have dropped one: {url}"
        );
    }

    #[test]
    fn the_island_filter_is_new_zealand_only() {
        let nz = Query::term("x")
            .island(Country::Nz, Some("SI"))
            .url(&endpoints(), "v");
        assert!(nz.contains("filters%5BAvailable%20in%20SI%5D=True"), "{nz}");

        // Australia has no such facet, and a caller should not have to branch.
        let au = Query::term("x").island(Country::Au, Some("SI"));
        assert!(au.filters.is_empty());
        // Neither should asking for no island do anything.
        assert!(Query::term("x")
            .island(Country::Nz, None)
            .filters
            .is_empty());
    }

    #[test]
    fn an_unknown_sort_is_passed_through_rather_than_refused() {
        assert_eq!(
            sort_for("price-low-to-high"),
            ("price".into(), "ascending".into())
        );
        assert_eq!(
            sort_for("Popular"),
            ("relevance".into(), "descending".into())
        );
        assert_eq!(sort_for("price"), ("price".into(), "ascending".into()));
        // The index publishes its own list per listing, so this must not fail.
        assert_eq!(
            sort_for("whatever"),
            ("whatever".into(), "descending".into())
        );
    }

    #[test]
    fn paging_is_one_based_and_the_page_size_is_bounded() {
        assert_eq!(Query::term("x").page(0).page, 1);
        assert_eq!(Query::term("x").per_page(0).per_page, 1);
        assert_eq!(Query::term("x").per_page(9999).per_page, 200);
    }

    #[test]
    fn a_listing_decodes_into_products() {
        let l = listing(&fixture("search-page.json")).unwrap();
        assert!(!l.products.is_empty());
        assert!(l.total > 0);
        let p = &l.products[0];
        assert!(!p.keycode.is_empty());
        assert!(!p.name.is_empty());
        assert!(p.price.is_some());
        assert_eq!(p.currency.as_deref(), Some("NZD"));
        assert!(!l.facets.is_empty(), "the refinements the caller can offer");
        assert!(!l.sorts.is_empty());
    }

    #[test]
    fn a_products_variations_each_keep_their_own_keycode() {
        let l = listing(&fixture("search-variations.json")).unwrap();
        let p = &l.products[0];
        assert!(p.variations.len() > 1);
        assert!(p.variations.iter().all(|v| !v.keycode.is_empty()));
        assert!(
            p.variations.iter().any(|v| v.keycode != p.keycode),
            "otherwise adding one to a cart would add the wrong thing"
        );
    }

    #[test]
    fn a_semantic_fallback_is_reported_as_a_guess() {
        let l = listing(&fixture("search-empty.json")).unwrap();
        assert!(!l.products.is_empty(), "the index answered anyway");
        assert!(l.is_guess(), "and none of it matched the words typed");
    }

    #[test]
    fn one_keycode_decodes_to_one_product_with_its_description() {
        let l = listing(&fixture("search-keycode.json")).unwrap();
        assert_eq!(l.products.len(), 1);
        let p = &l.products[0];
        assert_eq!(p.keycode, "43165537");
        assert!(p.description.is_some(), "the detail view needs this");
        assert!(p.rating.is_some());
        assert!(!p.marketplace(), "sold by Kmart");
    }

    #[test]
    fn the_tree_drops_the_synthetic_root_and_keeps_the_nesting() {
        let cats = categories(&fixture("browse-tree.json")).unwrap();
        assert!(!cats.is_empty());
        assert!(
            !cats.iter().any(|c| c.id == "all"),
            "`all` is a container, not a category"
        );
        let with_kids = cats.iter().find(|c| !c.children.is_empty()).unwrap();
        assert!(with_kids.path.is_some());
        assert!(with_kids.walk().len() > with_kids.children.len());
    }

    #[test]
    fn the_tree_url_asks_for_the_root_and_a_bounded_depth() {
        let url = tree_url(&endpoints(), "v", 99);
        assert!(url.contains("/browse/group_id/all?"), "{url}");
        assert!(url.contains("fmt_options%5Bgroups_max_depth%5D=6"), "{url}");
    }

    #[test]
    fn a_body_that_is_not_json_is_an_error_with_context() {
        let err = listing("<html>challenge</html>").unwrap_err();
        assert!(format!("{err}").contains("reading a listing"), "{err}");
    }
}
