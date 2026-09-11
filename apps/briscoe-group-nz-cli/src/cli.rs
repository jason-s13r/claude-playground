//! The flags, and nothing else. Parsing is separated from doing so that
//! `--help` is readable as one file and no command function has to know how it
//! was reached.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "bgnz",
    about = "Search and shop Briscoes and Rebel Sport New Zealand",
    version = crate::build::short_version(),
    long_version = crate::build::long_version(),
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Which fascia to talk to: briscoes or rebel.
    ///
    /// Defaults to `banner` in the config file, and to Briscoes when that is
    /// unset. The two share a backend but not a catalogue, a search index or a
    /// sign-in, so this is a switch rather than a fan-out -- there is nothing
    /// to compare between a jug and a football boot.
    #[arg(short = 'b', long = "banner", global = true, value_name = "BANNER")]
    pub banner: Option<String>,

    /// Print machine-readable JSON instead of a table.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// The command that would fix a failure, spelled for this binary.
pub fn advice(error: &crate::error::AppError) -> Option<String> {
    use bgnz_api::Error as Api;
    let crate::error::AppError::Api(api) = error else {
        return None;
    };
    Some(match api {
        Api::NotSignedIn { .. } | Api::SessionExpired { .. } => {
            "run `bgnz auth login` for this fascia".into()
        }
        // The one failure where "sign in again" is the *only* move: a refresh
        // has already been tried and refused.
        Api::LoginLapsed { .. } => "run `bgnz auth login` again, or `bgnz auth token`".into(),
        Api::CaptchaRequired { .. } => {
            "`bgnz auth login` drives a browser for you; `bgnz auth token` takes the \
             glt_ cookie out of one you have already signed in to"
                .into()
        }
        Api::NoSuchStore(_) => "run `bgnz stores` for the shops there are".into(),
        // Not "try again": the point of the message is that trying again
        // sooner is the wrong move.
        Api::RateLimited { .. } => "wait a few minutes before running this again".into(),
        _ => return None,
    })
}

/// The parser, in one place so `completions` generates for exactly what runs.
pub fn command() -> clap::Command {
    <Cli as clap::CommandFactory>::command()
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Search the catalogue.
    Search {
        query: String,
        #[command(flatten)]
        listing: Listing,
    },

    /// List a category's products.
    ///
    /// Takes a category id or uid; `bgnz categories` lists them.
    Browse {
        category: String,
        #[command(flatten)]
        listing: Listing,
    },

    /// The category tree.
    Categories {
        /// Show only categories whose name or path contains this.
        query: Option<String>,
        /// How deep to go. Level 2 is the top of the menu.
        #[arg(long, default_value_t = 3)]
        depth: i64,
    },

    /// One product: its price, its variants and what it is made of.
    Product {
        /// A SKU, or a product URL pasted from the site.
        sku: String,
    },

    /// Whether a product can be collected from a shop.
    ///
    /// A configurable product has no barcode of its own, so this answers per
    /// variant -- which on Rebel Sport means per size.
    Stock {
        /// A SKU, or a product URL pasted from the site.
        sku: String,
        /// The shop, by store id. Defaults to the one in the config file.
        #[arg(long, value_name = "STORE_ID")]
        store: Option<i64>,
    },

    /// The shops, with hours and collection times.
    Stores {
        /// Show only shops whose name, city or region contains this.
        query: Option<String>,
        /// Show only shops that do click and collect.
        #[arg(long)]
        collect: bool,
    },

    /// Set or show the shop `stock` uses when none is named.
    Store {
        #[command(subcommand)]
        action: StoreAction,
    },

    /// The basket.
    Cart {
        #[command(subcommand)]
        action: CartAction,
    },

    /// Saved items.
    Wishlist {
        #[command(subcommand)]
        action: WishlistAction,
    },

    /// What has been bought, online and in a shop.
    Orders {
        #[command(subcommand)]
        action: OrderAction,
    },

    /// Progress toward the next reward, and any vouchers earned.
    Loyalty {
        /// Ask the loyalty service rather than reading the storefront's cache.
        #[arg(long)]
        refresh: bool,
    },

    /// Credentials for one fascia.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Read and change the settings.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// What is set up, and what each fascia can do.
    Doctor,

    /// Update this binary.
    Update {
        /// A specific version, rather than the newest.
        version: Option<String>,
        /// Say what would happen, and change nothing.
        #[arg(long)]
        check: bool,
        #[arg(long)]
        pre_release: bool,
    },

    /// Write a shell completion script.
    Completions { shell: Option<clap_complete::Shell> },
}

/// The flags every product listing takes.
#[derive(clap::Args, Debug, Clone)]
pub struct Listing {
    /// How many products. The site's own page is 36.
    #[arg(long, value_name = "N")]
    pub limit: Option<u64>,
    /// Skip this many, for paging past the first screen.
    #[arg(long, default_value_t = 0, value_name = "N")]
    pub offset: u64,
    /// RELEVANCE, PRICE_ASC, PRICE_DESC, NAME_ASC, NAME_DESC, NEW_ARRIVAL.
    #[arg(long)]
    pub sort: Option<String>,
    /// Include products the index knows are out of stock. The site hides them.
    #[arg(long)]
    pub all: bool,
    /// Show the facets the index offered for narrowing this.
    #[arg(long)]
    pub facets: bool,
}

#[derive(Subcommand, Debug)]
pub enum StoreAction {
    /// Which shop is set.
    Show,
    /// Use this shop, by store id.
    Set { store: i64 },
    /// Forget it.
    Clear,
}

#[derive(Subcommand, Debug)]
pub enum CartAction {
    /// What is in it.
    List,
    /// Put something in it.
    Add {
        sku: String,
        #[arg(long, default_value_t = 1.0)]
        quantity: f64,
    },
    /// Change how many of a line there are. Takes the line's own id.
    Set { item: String, quantity: f64 },
    /// Take a line out. Takes the line's own id.
    Remove { item: String },
    /// Apply a discount code.
    Coupon { code: String },
    /// Whether the whole basket can be collected from a shop.
    ///
    /// One verdict for the lot, because that is what the service answers: the
    /// worst line in the basket decides it.
    Collect {
        #[arg(long, value_name = "STORE_ID")]
        store: Option<i64>,
    },
}

#[derive(Subcommand, Debug)]
pub enum WishlistAction {
    List,
    Add {
        sku: String,
        #[arg(long, default_value_t = 1.0)]
        quantity: f64,
    },
    /// Takes the item's own id, which `wishlist list` shows.
    Remove {
        item: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum OrderAction {
    /// Orders placed online.
    List {
        #[arg(long, default_value_t = 1)]
        page: u64,
        #[arg(long, default_value_t = 20)]
        limit: u64,
    },
    /// One online order, with its shipments and their tracking.
    Show { number: String },
    /// Receipts from walking into a shop.
    ///
    /// A different history from the online one, out of SAP rather than the
    /// storefront, and keyed to the loyalty membership. The service picks a
    /// rolling year unless told otherwise.
    Receipts {
        #[arg(long, default_value_t = 1)]
        page: u64,
        /// YYYYMMDD.
        #[arg(long, value_name = "YYYYMMDD")]
        from: Option<String>,
        /// YYYYMMDD.
        #[arg(long, value_name = "YYYYMMDD")]
        to: Option<String>,
    },
    /// A download link for an order's invoice.
    Invoice { number: String },
}

#[derive(Subcommand, Debug)]
pub enum AuthAction {
    /// Sign in, driving a browser for the captcha.
    ///
    /// Gigya refuses a sign-in without a reCAPTCHA token and refuses it before
    /// it looks at the password, so there is no way to do this with an HTTP
    /// client alone. The browser's whole job is to mint that token -- the email
    /// and password never reach it, and the sign-in itself happens here.
    ///
    /// Once only: what gets stored is Gigya's session token, and everything
    /// after this refreshes without a browser.
    Login {
        #[arg(long)]
        email: Option<String>,
        /// A command that prints the password, rather than typing it.
        #[arg(long, value_name = "COMMAND")]
        password_command: Option<String>,
        /// Show the browser window. Headless is the default and normally works;
        /// a window scores better if a headless run is refused.
        #[arg(long)]
        headful: bool,
    },

    /// Take a Gigya login token copied out of a signed-in browser.
    ///
    /// The way in that needs no browser here. On either site the token is a
    /// plain cookie -- `glt_<apiKey>`, 236 characters beginning `st2.s.` --
    /// so it can be read out of devtools' cookie list without touching the
    /// network tab. `bgnz auth token --cookie` prints which cookie to look for.
    Token {
        /// The token. Read from the terminal, hidden, if left off.
        token: Option<String>,
        /// Print the cookie name for this fascia and stop.
        #[arg(long)]
        cookie: bool,
    },

    /// Mint a fresh storefront token from the stored sign-in.
    ///
    /// Needs no browser and no password. Ordinary commands do this by
    /// themselves; this exists to check that they can.
    Refresh,

    /// What credentials are held, for both fascias.
    Status,

    /// Forget this fascia's credentials.
    Logout {
        /// Forget both fascias.
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Everything that is set.
    List,
    Get {
        key: String,
    },
    Set {
        key: String,
        value: String,
    },
    Unset {
        key: String,
    },
    /// Where the config and state files are.
    Path,
}
