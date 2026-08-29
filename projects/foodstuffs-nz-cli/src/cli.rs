//! Command-line surface.

use clap::{Args, Parser, Subcommand};

use crate::banner::Banner;
use crate::domain::order::Source;

#[derive(Parser, Debug)]
#[command(
    name = "fsnz",
    version,
    about = "Unofficial CLI for New World and PAK'nSAVE (Foodstuffs NZ)",
    long_about = "Search products, specials and stores at New World and PAK'nSAVE.\n\n\
Prices and stock are per store. Select a store before searching.\n\n  \
fsnz auth login --email you@example.com\n  \
fsnz stores wellington\n  \
fsnz store set \"NW Thorndon\"\n  \
fsnz search milk\n  \
fsnz compare milk\n\
fsnz orders list\n\n\
Not affiliated with Foodstuffs New Zealand. Calls undocumented endpoints; may break\n\
without notice."
)]
pub struct Cli {
    /// Banner: newworld/nw or paknsave/pns
    #[arg(short, long, global = true, env = "FSNZ_BANNER")]
    pub banner: Option<Banner>,

    /// Store to price against, overriding the saved one
    #[arg(long, global = true, value_name = "STORE_ID")]
    pub store: Option<String>,

    /// Token to use instead of minting one
    #[arg(long, global = true, env = "FSNZ_TOKEN", hide_env_values = true)]
    pub token: Option<String>,

    /// Emit JSON instead of formatted text
    #[arg(long, global = true)]
    pub json: bool,

    /// Absent when `fsnz` is run bare, which prints the help instead.
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Search for products
    Search {
        /// What to search for, e.g. "blue milk"
        query: String,
        #[command(flatten)]
        list: ListArgs,
        /// Only show products currently on special
        #[arg(long)]
        specials: bool,
    },

    /// List what is on special
    Specials {
        #[command(flatten)]
        list: ListArgs,
    },

    /// List a whole department, e.g. "Fruit & Vegetables"
    Browse {
        /// Top-level department name as the site spells it
        department: String,
        #[command(flatten)]
        list: ListArgs,
        /// Only show products currently on special
        #[arg(long)]
        specials: bool,
    },

    /// Compare a search across New World and PAK'nSAVE
    Compare {
        /// What to search for
        query: String,
        #[command(flatten)]
        list: ListArgs,
        /// Only compare products on special at one banner or the other
        #[arg(long)]
        specials: bool,
    },

    /// List stores, optionally filtered by name
    Stores {
        /// Only show stores whose name contains this
        query: Option<String>,
    },

    /// Show or choose the store to price against
    #[command(subcommand)]
    Store(StoreCommand),

    /// Manage the shopping cart; requires `fsnz auth login`
    #[command(subcommand)]
    Cart(CartCommand),

    /// Look through past orders; requires `fsnz auth login`
    #[command(subcommand)]
    Orders(OrdersCommand),

    /// Sign in to Club Plus, sign out, and inspect tokens
    #[command(subcommand)]
    Auth(AuthCommand),

    /// Check configuration, credentials and connectivity
    Doctor,
}

#[derive(Subcommand, Debug)]
pub enum AuthCommand {
    /// Log in through Club Plus
    Login {
        /// Club Plus email address; prompted for when omitted
        #[arg(long, env = "FSNZ_EMAIL")]
        email: Option<String>,
        /// Shell command printing the password on stdout. Overrides
        /// password_command in the config file.
        #[arg(long, value_name = "COMMAND")]
        password_command: Option<String>,
    },

    /// Forget the stored login and any cached tokens
    Logout,

    /// Show the token this tool would use
    Token {
        /// Mint a new token instead of reusing the cached one
        #[arg(long)]
        refresh: bool,
        /// Print the raw token only, for piping
        #[arg(long)]
        raw: bool,
    },

    /// Show the Club Plus session and each banner's token state
    Status,
}

#[derive(Subcommand, Debug)]
pub enum CartCommand {
    /// Show what is in the cart
    List,
    /// Add a product, or increase its quantity
    Add {
        /// Product SKU, as printed by `fsnz search`
        sku: String,
        /// Quantity; grams for weight-priced items. Defaults to 1 for counted items
        quantity: Option<u32>,
        /// Sale type, overriding the inference from the SKU
        #[arg(long, value_name = "units|weight")]
        unit: Option<String>,
    },
    /// Set a product's quantity outright
    Update {
        /// Product SKU
        sku: String,
        /// New quantity; grams for weight-priced items. Zero removes the line
        quantity: u32,
        /// Sale type, overriding the inference from the SKU
        #[arg(long, value_name = "units|weight")]
        unit: Option<String>,
    },
    /// Remove a product entirely
    Remove {
        /// Product SKU
        sku: String,
    },
    /// Empty the cart
    Clear {
        /// Required; cannot be undone
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum OrdersCommand {
    /// List past orders, most recent first
    List {
        /// Maximum orders to return
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=500))]
        limit: u32,
        /// Only show one kind of order: online or in-store
        #[arg(long, value_name = "online|in-store")]
        source: Option<Source>,
    },

    /// Show one order and what was in it
    Show {
        /// Position in `fsnz orders list`, or a whole order id
        #[arg(value_name = "POSITION_OR_ID")]
        order: String,
        /// Where the order came from, when the id alone does not say
        #[arg(long, value_name = "online|in-store")]
        source: Option<Source>,
    },

    /// List what this account has bought before, for buying it again
    Previous {
        /// Maximum products to return
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=500))]
        limit: u32,
        /// Keep products that are already in the cart, which are hidden by default
        #[arg(long)]
        include_cart: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum StoreCommand {
    /// Show the currently selected store
    Show,
    /// Select a store by id, or by a fragment of its name
    Set {
        /// Store id, or part of the store's name
        store: String,
    },
    /// Forget the selected store
    Clear,
}

/// Options shared by every command that returns a product list.
#[derive(Args, Debug, Clone)]
pub struct ListArgs {
    /// Maximum products to return
    #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=500))]
    pub limit: u32,

    /// Keep products whose size or name contains this, e.g. 2L
    #[arg(long)]
    pub size: Option<String>,

    /// Sort order, passed to the API verbatim
    #[arg(long, default_value = crate::api::DEFAULT_SORT)]
    pub sort: String,
}
