//! Who we are talking to, what they can do, and the trait an adapter fills in.

use serde::{Deserialize, Serialize};

use crate::cart::{Cart, Change};
use crate::department::Department;
use crate::error::{Error, Result};
use crate::order::{Order, OrderFilter, OrderLine, OrderSummary};
use crate::search::{Search, SearchResult};
use crate::store::Store;

/// The three storefronts. New World and PAK'nSAVE are one platform wearing two
/// names; Woolworths is a different company, a different protocol and a
/// different catalogue.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RetailerId {
    NewWorld,
    PaknSave,
    Woolworths,
}

impl RetailerId {
    pub const ALL: [RetailerId; 3] = [
        RetailerId::NewWorld,
        RetailerId::PaknSave,
        RetailerId::Woolworths,
    ];

    /// The stable machine name: config tables, state directories, JSON.
    pub fn id(self) -> &'static str {
        match self {
            RetailerId::NewWorld => "newworld",
            RetailerId::PaknSave => "paknsave",
            RetailerId::Woolworths => "woolworths",
        }
    }

    /// What `-b` takes and what error messages tell people to type.
    pub fn short(self) -> &'static str {
        match self {
            RetailerId::NewWorld => "nw",
            RetailerId::PaknSave => "pns",
            RetailerId::Woolworths => "ww",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RetailerId::NewWorld => "New World",
            RetailerId::PaknSave => "PAK'nSAVE",
            RetailerId::Woolworths => "Woolworths",
        }
    }

    /// Retailers that share one catalogue, and so can be joined on SKU.
    /// `None` means "nothing else uses these product codes".
    pub fn catalogue(self) -> Option<&'static str> {
        match self {
            RetailerId::NewWorld | RetailerId::PaknSave => Some("foodstuffs"),
            RetailerId::Woolworths => None,
        }
    }
}

impl std::fmt::Display for RetailerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Rejected spellings say what was accepted rather than just failing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown retailer {0:?}: try nw, pns or ww")]
pub struct ParseRetailerError(pub String);

impl std::str::FromStr for RetailerId {
    type Err = ParseRetailerError;

    /// Deliberately generous. People type `PAK'nSAVE`, `pak n save`, `Pack n
    /// Save` and `paknsave` for the same shop, and `countdown` for the one that
    /// was renamed. Folding to bare alphanumerics absorbs all of it.
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let folded: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect();
        match folded.as_str() {
            "nw" | "newworld" => Ok(RetailerId::NewWorld),
            "pns" | "pak" | "pakn" | "paknsave" | "packnsave" | "pakcnsave" => {
                Ok(RetailerId::PaknSave)
            }
            "ww" | "woolworths" | "woolies" | "countdown" | "cd" => Ok(RetailerId::Woolworths),
            _ => Err(ParseRetailerError(s.to_string())),
        }
    }
}

/// What a retailer can actually do.
///
/// Every field here is a command that exists for at least one retailer and not
/// for another. The dispatcher reads this *before* doing network work, and
/// `doctor` prints it, so a gap is something you are told about rather than
/// something you discover by hitting an error.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Caps {
    pub departments: bool,
    pub order_detail: bool,
    pub previous_purchases: bool,
    pub refresh_session: bool,
    pub import_cookies: bool,
    /// Lines can be priced by the kilogram rather than the unit.
    pub weight_lines: bool,
    /// Selecting a store is a server-side mutation, not just a local preference.
    pub server_side_store: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthStatus {
    pub retailer: RetailerId,
    pub signed_in: bool,
    pub account: Option<String>,
    /// Seconds until the credential stops working, when that is knowable.
    pub expires_in: Option<u64>,
    pub detail: Option<String>,
}

/// One supermarket, as the rest of the program sees it.
///
/// Implementors live in the app, wrapping a vendor API crate and converting its
/// types to these. The capability-gated methods default to a typed refusal:
/// a caller that skips [`Caps`] still fails honestly instead of getting an
/// empty result that looks like "you have no orders".
#[async_trait::async_trait]
pub trait Retailer: Send + Sync {
    fn id(&self) -> RetailerId;
    fn caps(&self) -> Caps;

    async fn search(&self, search: &Search) -> Result<SearchResult>;
    async fn stores(&self, query: Option<&str>, max: u32) -> Result<Vec<Store>>;
    /// Resolve a store, and bind it server-side where the retailer requires
    /// that. Persisting the choice is the caller's job, and is the same for all
    /// three.
    async fn select_store(&self, id: &str) -> Result<Store>;
    async fn cart(&self) -> Result<Cart>;
    async fn cart_apply(&self, changes: &[Change]) -> Result<Cart>;
    async fn cart_clear(&self) -> Result<Cart>;
    async fn orders(&self, filter: OrderFilter, max: u32) -> Result<Vec<OrderSummary>>;
    async fn auth_status(&self) -> Result<AuthStatus>;
    async fn logout(&self) -> Result<bool>;

    async fn departments(&self) -> Result<Vec<Department>> {
        Err(Error::unsupported(self.id(), "departments"))
    }

    async fn order(&self, _id: &str) -> Result<Order> {
        Err(Error::unsupported(self.id(), "order detail"))
    }

    async fn previous_purchases(&self, _max: u32, _exclude_cart: bool) -> Result<Vec<OrderLine>> {
        Err(Error::unsupported(self.id(), "previous purchases"))
    }

    async fn refresh_session(&self) -> Result<AuthStatus> {
        Err(Error::unsupported(self.id(), "session refresh"))
    }

    async fn import_cookies(&self, _text: &str) -> Result<AuthStatus> {
        Err(Error::unsupported(self.id(), "cookie import"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_spellings_people_actually_type() {
        for (input, want) in [
            ("nw", RetailerId::NewWorld),
            ("New World", RetailerId::NewWorld),
            ("new-world", RetailerId::NewWorld),
            ("pns", RetailerId::PaknSave),
            ("PAK'nSAVE", RetailerId::PaknSave),
            ("pak n save", RetailerId::PaknSave),
            ("Pack n Save", RetailerId::PaknSave),
            ("ww", RetailerId::Woolworths),
            ("woolies", RetailerId::Woolworths),
            ("Countdown", RetailerId::Woolworths),
        ] {
            assert_eq!(input.parse::<RetailerId>().unwrap(), want, "{input}");
        }
    }

    #[test]
    fn rejects_an_unknown_shop_by_name() {
        let err = "tesco".parse::<RetailerId>().unwrap_err();
        assert!(err.to_string().contains("tesco"), "{err}");
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        // These land in config keys and state directory names; changing one
        // silently orphans a user's saved store.
        let ids: Vec<&str> = RetailerId::ALL.iter().map(|r| r.id()).collect();
        assert_eq!(ids, ["newworld", "paknsave", "woolworths"]);
    }

    #[test]
    fn only_the_foodstuffs_banners_share_a_catalogue() {
        assert_eq!(RetailerId::NewWorld.catalogue(), Some("foodstuffs"));
        assert_eq!(RetailerId::PaknSave.catalogue(), Some("foodstuffs"));
        assert_eq!(RetailerId::Woolworths.catalogue(), None);
    }
}
