//! Command-line surface.

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::domain::order::Filter;

/// Built rather than written out so the list cannot drift from
/// [`crate::api::SORTS`], which is what the API actually accepts.
fn sort_help() -> String {
    format!("Sort order: {}", crate::api::SORTS.join(", "))
}

#[derive(Parser, Debug)]
#[command(
    name = "wwnz",
    version = crate::build::short_version(),
    long_version = crate::build::long_version(),
    about = "Unofficial CLI for Woolworths NZ",
    long_about = "Search products, specials and stores at Woolworths New Zealand.\n\n\
Prices and stock are per store. Select a store before searching.\n\n  \
wwnz stores whangarei\n  \
wwnz store set \"Regent Woolworths\"\n  \
wwnz search milk\n  \
wwnz specials --limit 50\n\n\
The cart and order history need an account:\n\n  \
wwnz auth login --email you@example.com\n  \
wwnz cart add 282768\n  \
wwnz orders list\n\n\
Not affiliated with Woolworths New Zealand. Calls undocumented endpoints; may break\n\
without notice."
)]
pub struct Cli {
    /// Store to price against, overriding the saved one
    #[arg(long, global = true, value_name = "STORE_ID", env = "WWNZ_STORE_ID")]
    pub store: Option<String>,

    /// Emit JSON instead of formatted text
    #[arg(long, global = true)]
    pub json: bool,

    /// Absent when `wwnz` is run bare, which prints the help instead.
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

    /// List a whole department, e.g. "Fruit & Veg"
    Browse {
        /// Department, aisle or shelf, by name, slug or category key.
        /// `wwnz departments` lists them.
        department: String,
        #[command(flatten)]
        list: ListArgs,
        /// Only show products currently on special
        #[arg(long)]
        specials: bool,
    },

    /// List the department tree `wwnz browse` selects from
    Departments {
        /// Only show departments whose name contains this
        query: Option<String>,
        /// How deep to go: 1 departments, 2 aisles, 3 shelves
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(1..=3))]
        depth: u32,
    },

    /// List stores, optionally filtered by name or town
    Stores {
        /// Only show stores whose name, suburb or address contains this
        query: Option<String>,
        /// Maximum stores to return
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(1..=500))]
        limit: u32,
    },

    /// Show or choose the store to price against
    #[command(subcommand)]
    Store(StoreCommand),

    /// Manage the shopping cart; requires `wwnz auth login`
    #[command(subcommand)]
    Cart(CartCommand),

    /// Look through past orders; requires `wwnz auth login`
    #[command(subcommand)]
    Orders(OrdersCommand),

    /// Sign in, sign out, and inspect the stored session
    #[command(subcommand)]
    Auth(AuthCommand),

    /// Check configuration, credentials and connectivity
    Doctor,

    /// Print a shell completion script for wwnz
    Completions {
        /// Shell to generate for; inferred from $SHELL when omitted
        #[arg(value_name = "SHELL")]
        shell: Option<Shell>,
    },

    /// Check for a newer release of wwnz, and install it
    Update {
        /// Version to install, e.g. `0.1.4-rc.2`; a leading `v` is optional.
        /// Installs exactly that release, downgrades included.
        version: Option<String>,
        /// Report what is available without installing anything. Exits
        /// non-zero when there is a newer release, so it can gate a script.
        #[arg(long)]
        check: bool,
        /// Take the newest release even when it is a preview.
        #[arg(long)]
        pre_release: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum AuthCommand {
    /// Sign in with an email address and password
    Login {
        /// Account email address; prompted for when omitted
        #[arg(long, env = "WWNZ_EMAIL")]
        email: Option<String>,
        /// Shell command printing the password on stdout. Overrides
        /// password_command in the config file.
        #[arg(long, value_name = "COMMAND")]
        password_command: Option<String>,
    },

    /// Take the session from cookies exported from a browser
    ///
    /// The way in when the login flow cannot be followed -- a verification
    /// step, or a bot check. Sign in with a browser, export its cookies for
    /// woolworths.co.nz, and hand the file over.
    Import {
        /// A Netscape-format cookies.txt. Reads stdin when this is `-`.
        #[arg(value_name = "COOKIES_FILE")]
        file: String,
    },

    /// Forget the stored session and any cached tokens
    Logout,

    /// Show whether there is a session, and whether it still works
    Status,
}

#[derive(Subcommand, Debug)]
pub enum CartCommand {
    /// Show what is in the cart
    List,
    /// Add a product, or increase its quantity
    Add {
        /// Product SKU, as printed by `wwnz search`
        sku: String,
        /// Quantity to add; defaults to 1
        quantity: Option<u32>,
        /// Unit suffix, when the SKU alone does not say (EA, KGM)
        #[arg(long, value_name = "UNIT")]
        unit: Option<String>,
    },
    /// Set a product's quantity outright
    Update {
        /// Product SKU
        sku: String,
        /// New quantity. Zero removes the line
        quantity: u32,
        /// Unit suffix, when the SKU alone does not say (EA, KGM)
        #[arg(long, value_name = "UNIT")]
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
    /// List orders, most recent first
    ///
    /// Only the list is available. The operation the website uses for one
    /// order's contents was not present in the traffic this tool was built
    /// from, and is not guessed at here.
    List {
        /// Maximum orders to return
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=500))]
        limit: u32,
        /// Which orders to include
        #[arg(long, default_value = "all", value_name = "active|past|all")]
        filter: Filter,
    },

    /// List what this account has bought before, for buying it again
    Previous {
        #[command(flatten)]
        list: ListArgs,
    },
}

#[derive(Subcommand, Debug)]
pub enum StoreCommand {
    /// Show the currently selected store
    Show,
    /// Select a store by id, or by a fragment of its name or town
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

    /// Keep products whose name contains this, e.g. 2L
    #[arg(long)]
    pub size: Option<String>,

    /// Sort order. Passed to the API verbatim, so a value not listed here
    /// still reaches it.
    #[arg(long, value_name = "SORT", help = sort_help())]
    pub sort: Option<String>,
}
