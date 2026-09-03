//! The flags, and nothing else. Parsing is separated from doing so that
//! `--help` is readable as one file and no command function has to know how it
//! was reached.

use clap::{Parser, Subcommand};

use gsnz_core::{OrderFilter, RetailerId, Sort};

#[derive(Parser, Debug)]
#[command(
    name = "gsnz",
    about = "Search, compare and shop New World, PAK'nSAVE and Woolworths NZ",
    version = crate::build::short_version(),
    long_version = crate::build::long_version(),
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Which shop to talk to: nw, pns or ww.
    ///
    /// Defaults to `retailer` in the config file. `compare` takes a list --
    /// `-b nw,pns` -- and every other command takes exactly one.
    #[arg(
        short = 'b',
        long = "retailer",
        global = true,
        env = "GSNZ_RETAILER",
        value_delimiter = ','
    )]
    pub retailer: Vec<RetailerId>,

    /// Use this store for this command only, without saving it.
    #[arg(long, global = true, value_name = "ID")]
    pub store: Option<String>,

    /// Use this bearer token instead of acquiring one. Foodstuffs only.
    #[arg(
        long,
        global = true,
        value_name = "TOKEN",
        env = "GSNZ_TOKEN",
        hide_env_values = true
    )]
    pub token: Option<String>,

    /// Print machine-readable JSON instead of a table.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Search a shop's catalogue.
    Search {
        query: String,
        #[command(flatten)]
        listing: Listing,
    },

    /// Everything currently on promotion.
    Specials {
        #[command(flatten)]
        listing: Listing,
    },

    /// List a department's products.
    ///
    /// Takes a department name; `gsnz departments` lists them.
    Browse {
        department: String,
        #[command(flatten)]
        listing: Listing,
    },

    /// The department tree.
    Departments {
        /// Show only the subtree under a department.
        query: Option<String>,
        /// How many levels to print.
        #[arg(long, default_value_t = 2)]
        depth: u32,
    },

    /// Find a store.
    Stores {
        /// Filter by name, suburb or city.
        query: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },

    /// Set the shop that commands use when `-b` is not given.
    ///
    /// Shorthand for `gsnz config set retailer <SHOP>`. With no argument it
    /// says which shop is current.
    Use { retailer: Option<RetailerId> },

    /// Read and change the settings file.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show or change which store prices are quoted against.
    Store {
        #[command(subcommand)]
        action: StoreAction,
    },

    /// The same products at every shop, side by side.
    ///
    /// Spans all three unless `-b` names fewer. Rows matched by description
    /// rather than by product code are marked; `--strict` drops them.
    Compare {
        query: String,
        #[command(flatten)]
        listing: Listing,
        /// Pair only products that share a product code, which across
        /// catalogues means Woolworths appears in its own rows.
        #[arg(long)]
        strict: bool,
    },

    /// What is in the shopping cart, and changing it.
    Cart {
        #[command(subcommand)]
        action: CartAction,
    },

    /// Past orders.
    Orders {
        #[command(subcommand)]
        action: OrderAction,
    },

    /// Signing in, and the four ways a session ends.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// What is set up, and what each shop can do.
    Doctor,

    /// Replace this binary with a newer release.
    Update {
        /// A specific version, rather than the newest.
        version: Option<String>,
        /// Report what is available without installing it.
        #[arg(long)]
        check: bool,
        /// Consider pre-releases.
        #[arg(long)]
        pre_release: bool,
    },

    /// Print a shell completion script.
    Completions {
        /// bash, zsh, fish, elvish or powershell. Guessed from $SHELL if left off.
        shell: Option<String>,
    },
}

/// The flags every product listing takes. `search`, `specials` and `browse`
/// differ only in what selects the products, so they share this.
#[derive(clap::Args, Debug, Clone)]
pub struct Listing {
    /// How many products to return.
    #[arg(long, default_value_t = 20)]
    pub limit: u32,

    /// Keep only products whose size matches, e.g. `2l`, `500g`.
    #[arg(long)]
    pub size: Option<String>,

    /// relevance, popularity, price-asc, price-desc or name-asc.
    #[arg(long, default_value = "relevance")]
    pub sort: Sort,

    /// Keep only products on promotion.
    #[arg(long)]
    pub specials: bool,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Every setting, its value, and what it does.
    List,
    /// Print one value, and nothing else.
    Get { key: String },
    /// Change one value. Refused now if it will not parse.
    Set { key: String, value: String },
    /// Put one setting back to its default.
    Unset { key: String },
    /// Where the file is.
    Path,
}

#[derive(Subcommand, Debug)]
pub enum AuthAction {
    /// Sign in.
    Login {
        #[arg(long)]
        email: Option<String>,
        /// A command that prints the password, for a password manager.
        #[arg(long)]
        password_command: Option<String>,
        /// Do not keep the password. A Woolworths session then cannot be
        /// renewed at all, since its cookie is not refreshable.
        #[arg(long)]
        no_store_password: bool,
    },
    /// Seed a session from a browser's Netscape cookies.txt.
    Import {
        #[arg(value_name = "COOKIES_FILE")]
        file: std::path::PathBuf,
    },
    /// Renew the session without a full sign-in, where that is possible.
    Refresh,
    /// Who is signed in, and for how much longer.
    Status,
    /// Forget the session, the cookies and any stored password.
    Logout,
}

#[derive(Subcommand, Debug)]
pub enum CartAction {
    /// What is in it.
    List,
    /// Add to a line, or start one.
    Add {
        sku: String,
        /// How many, or how many kilograms with `--unit kg`.
        #[arg(default_value = "1")]
        quantity: f64,
        #[command(flatten)]
        unit: Unit,
    },
    /// Set a line to an exact quantity. Zero removes it.
    Update {
        sku: String,
        quantity: f64,
        #[command(flatten)]
        unit: Unit,
    },
    /// Take a line out.
    Remove { sku: String },
    /// Empty it.
    Clear {
        /// Required: this cannot be undone.
        #[arg(long)]
        force: bool,
    },
}

/// How a quantity is counted, where the product code does not already say.
#[derive(clap::Args, Debug, Clone)]
pub struct Unit {
    /// The quantity is kilograms rather than a count.
    #[arg(long = "unit", value_name = "kg", num_args = 0..=1, default_missing_value = "kg")]
    pub kg: Option<String>,
}

impl Unit {
    pub fn is_weight(&self) -> bool {
        self.kg.is_some()
    }
}

#[derive(Subcommand, Debug)]
pub enum OrderAction {
    /// Recent orders, newest first.
    List {
        #[arg(long, default_value_t = 20)]
        limit: u32,
        /// all, active, past, online or in-store.
        #[arg(long, default_value = "all")]
        filter: OrderFilter,
    },
    /// One order and what was in it.
    ///
    /// Takes an order id, or its position in `orders list`.
    Show { order: String },
    /// Products bought before, for restocking.
    Previous {
        #[arg(long, default_value_t = 20)]
        limit: u32,
        /// Include products already in the cart.
        #[arg(long)]
        include_cart: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum StoreAction {
    /// What is selected now.
    Show,
    /// Select a store by id, or by enough of its name to be unambiguous.
    Set { store: String },
    /// Forget the selected store.
    Clear,
}
