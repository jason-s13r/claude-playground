//! The flags, and nothing else. Parsing is separated from doing so that
//! `--help` is readable as one file and no command function has to know how it
//! was reached.

use clap::{Parser, Subcommand};

use gsnz_core::{RetailerId, Sort};

#[derive(Parser, Debug)]
#[command(
    name = "gsnz",
    about = "Search, compare and shop New World, PAK'nSAVE and Woolworths NZ",
    version = crate::build::short_version(),
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Which shop to talk to: nw, pns or ww.
    ///
    /// Defaults to `retailer` in the config file. `compare` spans all three
    /// regardless unless given a list.
    #[arg(short = 'b', long = "retailer", global = true, env = "GSNZ_RETAILER")]
    pub retailer: Option<RetailerId>,

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

    /// Show or change which store prices are quoted against.
    Store {
        #[command(subcommand)]
        action: StoreAction,
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
pub enum StoreAction {
    /// What is selected now.
    Show,
    /// Select a store by id, or by enough of its name to be unambiguous.
    Set { store: String },
    /// Forget the selected store.
    Clear,
}
