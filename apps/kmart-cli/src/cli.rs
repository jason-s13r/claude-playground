//! The flags, and nothing else. Parsing is separated from doing so that
//! `--help` is readable as one file and no command function has to know how it
//! was reached.

use clap::{Parser, Subcommand};
use kmart_api::Country;

#[derive(Parser, Debug)]
#[command(
    name = "kmart",
    about = "Search and shop Kmart Australia and New Zealand",
    version = crate::build::short_version(),
    long_version = crate::build::long_version(),
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Which country to ask, for this command only: `au` or `nz`.
    ///
    /// Not a display preference -- it selects the catalogue, the prices and
    /// the currency. `kmart use` sets the lasting one, and Australia is what
    /// is asked until something does.
    ///
    /// For one command, with one exception: `auth login` keeps the country it
    /// signed in to, because the session is only good against that storefront.
    #[arg(long, short = 'c', global = true, value_name = "au|nz")]
    pub country: Option<Country>,

    /// Print machine-readable JSON instead of a table.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// The command that would fix a failure, spelled for this binary.
pub fn advice(error: &crate::error::AppError) -> Option<&'static str> {
    let crate::error::AppError::Api(api) = error else {
        return None;
    };
    Some(match api {
        kmart_api::Error::NotSignedIn => "run `kmart auth login`",
        kmart_api::Error::SessionExpired | kmart_api::Error::SessionUnrenewable => {
            "the refresh token is gone or was refused; run `kmart auth login` again"
        }
        // The email and password are worth re-checking only when Auth0 was
        // the one that said no. It refuses the refresh token for a different
        // reason -- an old or truncated paste -- and Akamai, which refuses the
        // password submit outright, is reported as a challenge instead.
        // Rotation makes this the least guessable failure here: the token was
        // valid when it was obtained, and something else has spent it since --
        // the browser it came from, or another copy of this session.
        kmart_api::Error::LoginRefused { step, .. } if *step == "refresh token" => {
            "each use invalidates the last, so something has already spent this one; \
             run `kmart auth login` for a fresh session"
        }
        kmart_api::Error::LoginRefused { .. } => "check the email and password, then try again",
        // The one failure in this program that cannot be fixed by trying
        // harder, so it gets the longest answer.
        kmart_api::Error::Challenged { .. } | kmart_api::Error::NoSession { .. } => {
            "run `kmart auth login`, which earns the bot-check cookies and an \
             account token in one go; failing that, export cookies from a \
             signed-in browser for `kmart auth import <cookies.txt>`"
        }
        kmart_api::Error::NoSuchStore(_) => "run `kmart stores` for the ones nearby",
        kmart_api::Error::NoSuchCategory(_) => "run `kmart categories` for the ones there are",
        kmart_api::Error::CartConflict => "run the command again; it reads the cart first",
        // Not "try again": the point of the message is that trying again
        // sooner is the wrong move.
        kmart_api::Error::RateLimited { .. } => "wait a few minutes before running this again",
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
    /// Takes a category id or enough of its name to be unambiguous;
    /// `kmart categories` lists them.
    Browse {
        category: String,
        #[command(flatten)]
        listing: Listing,
    },

    /// The category tree.
    Categories {
        /// Show only the subtree under a category.
        query: Option<String>,
        /// How many levels to show.
        #[arg(long, default_value_t = 2)]
        depth: u32,
    },

    /// One product: its price, its variations and where it is in stock.
    Product {
        /// A keycode, e.g. `43165537`.
        keycode: String,
        /// Skip the stock lookup, which needs imported cookies.
        #[arg(long)]
        no_stock: bool,
    },

    /// Where a product can be had, near a postcode.
    Stock {
        keycode: String,
        /// Ask about this postcode for this command only. Defaults to the one
        /// `kmart postcode` holds.
        #[arg(long)]
        postcode: Option<String>,
    },

    /// The stores nearest a postcode.
    Stores {
        /// Filter the results by name, suburb or city.
        query: Option<String>,
        /// Look near this postcode rather than the configured one.
        #[arg(long)]
        postcode: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },

    /// Show or change the postcode stock is quoted for.
    ///
    /// Every stock answer is relative to one, so this is the setting that
    /// makes `stock` and `stores` work.
    Postcode {
        #[command(subcommand)]
        action: Option<PostcodeAction>,
    },

    /// Show or change which island New Zealand listings are filtered to.
    ///
    /// Kmart ranges differently across the strait, so this changes what a
    /// listing contains rather than only how it is shown. Australia has no
    /// equivalent and ignores it.
    Island {
        #[command(subcommand)]
        action: Option<IslandAction>,
    },

    /// Set the country commands use when `--country` is not given.
    ///
    /// Australia until this says otherwise -- or until `auth login`, which
    /// writes down the storefront it signed in to.
    ///
    /// Shorthand for `kmart config set country <COUNTRY>`. With no argument it
    /// says which one is current.
    Use { country: Option<Country> },

    /// What is in the shopping bag, and changing it. Needs an account.
    Cart {
        #[command(subcommand)]
        action: Option<CartAction>,
    },

    /// What is saved for later, and changing it. Needs an account.
    ///
    /// With nothing after it, this shows the list -- the reading is what a
    /// wishlist is for, so it does not need naming.
    Wishlist {
        #[command(subcommand)]
        action: Option<WishlistAction>,
    },

    /// Past orders. Needs an account.
    Orders {
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },

    /// Signing in, importing cookies, and signing out.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Read and change the settings file.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// What is set up, and what works.
    Doctor,

    /// Replace this binary with a newer release.
    Update {
        /// A specific version, rather than the newest.
        version: Option<String>,
        /// Report what is available, and its release notes, without installing
        /// it.
        #[arg(long)]
        check: bool,
        /// Consider pre-releases.
        #[arg(long)]
        pre_release: bool,
    },

    /// Print a shell completion script.
    Completions {
        /// bash, zsh, fish, elvish or powershell. Guessed from $SHELL if left
        /// off.
        shell: Option<String>,
    },
}

/// The flags a product listing takes. `search` and `browse` differ only in
/// what selects the products, so they share this.
#[derive(clap::Args, Debug, Clone)]
pub struct Listing {
    /// How many products to return.
    #[arg(long, default_value_t = 20)]
    pub limit: u32,

    /// Which page of them.
    #[arg(long, default_value_t = 1)]
    pub page: u32,

    /// How to order the results: `popular`, `new`, `price-low-to-high`,
    /// `price-high-to-low`. An unfamiliar value is passed through rather than
    /// refused, because the index publishes its own list per listing.
    #[arg(long)]
    pub sort: Option<String>,

    /// Keep only one brand.
    #[arg(long)]
    pub brand: Option<String>,

    /// Keep only one colour.
    #[arg(long)]
    pub color: Option<String>,

    /// Keep only one size.
    #[arg(long)]
    pub size: Option<String>,

    /// Any other refinement, as `NAME=VALUE`. Repeatable.
    ///
    /// Kmart's facets differ per category -- `Material`, `Capacity`,
    /// `Power Rating` -- so a flag per facet would be wrong. Run a listing and
    /// read the refinements it reports.
    #[arg(long = "filter", value_name = "NAME=VALUE")]
    pub filters: Vec<String>,

    /// Use this island for this command only, without saving it. New Zealand
    /// only. See `kmart island`.
    #[arg(long, value_name = "north|south")]
    pub island: Option<String>,

    /// Show the refinements this listing offers, and stop.
    #[arg(long)]
    pub facets: bool,
}

impl Listing {
    /// The refinements these flags amount to, in the order they are sent.
    ///
    /// The named flags first, then `--filter`, so a repeated facet keeps the
    /// order it was typed in.
    pub fn filters(&self) -> Result<Vec<(String, String)>, String> {
        let mut out = Vec::new();
        for (name, value) in [
            ("Brand", self.brand.as_deref()),
            ("Colour", self.color.as_deref()),
            ("Size", self.size.as_deref()),
        ] {
            if let Some(value) = value {
                out.push((name.to_string(), value.to_string()));
            }
        }
        for raw in &self.filters {
            let (name, value) = raw
                .split_once('=')
                .ok_or_else(|| format!("{raw:?} is not NAME=VALUE"))?;
            if name.trim().is_empty() {
                return Err(format!("{raw:?} has no facet name"));
            }
            out.push((name.trim().to_string(), value.trim().to_string()));
        }
        Ok(out)
    }
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
    /// Sign in with an email and a password, through a browser.
    ///
    /// Kmart's bot check guards the password submit and refuses any plain HTTP
    /// client, so this drives a real browser to do it -- `camoufox`, which has
    /// to be installed separately:
    ///
    ///     uv tool install "camoufox[geoip]" && camoufox fetch
    ///
    /// It runs without a window by default; pass `--headful` to watch it, which
    /// is also the stronger path when the bot check is being stubborn.
    ///
    /// One run yields both credentials, the token and the cookies, so it
    /// replaces `auth token` and `auth import` together. Those two remain for
    /// when there is no browser to hand.
    Login {
        /// The account to sign in as. Asked for if left off.
        #[arg(long)]
        email: Option<String>,
        /// A command that prints the password, for a password manager.
        ///
        /// e.g. `--password-command 'op read "op://Vault/Kmart/password"'`.
        /// The password never reaches this program's arguments or output.
        #[arg(long)]
        password_command: Option<String>,
        /// Do not keep the password.
        #[arg(long)]
        no_store_password: bool,
        /// Show the browser window.
        ///
        /// The default is headless, which passes the bot check in the common
        /// case. Headful has more input entropy for the sensor to score, so it
        /// is the one to reach for when a headless run is refused -- a refusal
        /// is reported as a challenge rather than as a wrong password.
        #[arg(long)]
        headful: bool,
        /// Sign in with no browser at all, replaying Auth0's login as direct
        /// HTTP requests.
        ///
        /// A diagnostic, not a way in. It walks the exact flow the browser
        /// walks, made instead by this program's own emulation client, and
        /// prints each step to stderr so where it stops is visible. Against the
        /// live site Akamai answers the password submit with a challenge --
        /// admission is bound to the browser that ran its sensor, and this is
        /// not that browser -- so it is here to measure that wall, and to
        /// notice the day it moves. It earns no bot-check cookies.
        #[arg(long, conflicts_with = "headful")]
        direct: bool,
    },
    /// Take the bot-check cookies out of a browser export.
    ///
    /// Kmart's gateway is behind Akamai Bot Manager, which answers anything
    /// that has not run its sensor script. There is no way around that from a
    /// command line, so the cookies come from a browser that already passed
    /// it. They last about a day. Only the bot-check cookies are kept.
    Import {
        /// A Netscape-format `cookies.txt`, as browser export extensions and
        /// `curl -c` write. `-` reads standard input.
        file: String,
    },
    /// Take an Auth0 refresh token, copied out of a signed-in browser.
    ///
    /// The way in that works. Kmart's token endpoint is *not* bot-checked, so
    /// one refresh token renews a session indefinitely -- no password, no
    /// re-import, unlike the cookies beside it.
    ///
    /// In devtools on kmart.co.nz or kmart.com.au, look under Local Storage
    /// for the key beginning `@@auth0spajs@@` and copy its `refresh_token`.
    Token {
        /// The token. Read from the terminal, hidden, if left off -- which
        /// also keeps it out of the shell history.
        token: Option<String>,
    },
    /// Renew whatever has lapsed, without typing anything.
    ///
    /// For a cron job or a wrapper script: it does what `auth login` does, but
    /// from the email and the password already on hand, and only as far as it
    /// has to go.
    ///
    /// Cheapest first. The token endpoint is not bot-checked, so a good refresh
    /// token renews with no browser involved. The cookies have no clock to read
    /// -- a stale `_abck` looks exactly like a good one -- so they are tested
    /// by spending one gateway request, and only a refusal opens a browser.
    /// That browser run earns both credentials again, as `auth login` does.
    ///
    /// An account with neither a stored password nor an `auth.password_command`
    /// cannot get past a refused token, and says so with exit code 3.
    Refresh {
        /// Show the browser window, when a browser is needed at all.
        ///
        /// As with `auth login`: headless passes the bot check in the common
        /// case, and `--headful` gives the sensor more to score when it does
        /// not. A refresh that stays on the token endpoint never opens one.
        #[arg(long)]
        headful: bool,
        /// Renew both halves whatever state they are in.
        ///
        /// The default stops as soon as the session is proven good, which is
        /// what a scheduled run wants -- Auth0 rotates the refresh token on
        /// every use, so renewing one that has not lapsed is a rotation for
        /// nothing. This spends it anyway and signs in through a browser
        /// regardless, for a session that is misbehaving in some way the
        /// gateway does not report as a refusal.
        #[arg(long)]
        force: bool,
        /// Sign in with no browser at all, when the refresh token fails.
        ///
        /// The same diagnostic as `auth login --direct`: it replays Auth0's
        /// login as direct HTTP requests, narrating each step to stderr. The
        /// live bot check answers the password submit with a challenge, so
        /// this is here to measure that wall, not to get past it.
        #[arg(long, conflicts_with = "headful")]
        direct: bool,
    },
    /// Who is signed in, whether the gateway will answer, and until when.
    Status,
    /// Forget the session, the cookies and any stored password.
    Logout,
}

#[derive(Subcommand, Debug)]
pub enum CartAction {
    /// What is in it. The same as naming nothing at all.
    List,
    /// Add a product.
    Add {
        keycode: String,
        #[arg(default_value_t = 1)]
        quantity: i64,
    },
    /// Set a line to an exact quantity. Zero removes it.
    Set { keycode: String, quantity: i64 },
    /// Take a line out.
    Remove { keycode: String },
}

#[derive(Subcommand, Debug)]
pub enum WishlistAction {
    /// What is saved. The same as naming nothing at all.
    List,
    /// Save a product.
    Add {
        keycode: String,
        #[arg(default_value_t = 1)]
        quantity: i64,
    },
}

#[derive(Subcommand, Debug)]
pub enum PostcodeAction {
    /// Which postcode is set now. The same as naming nothing at all.
    Show,
    /// Look one up, to check it before setting it.
    Find { query: String },
    /// Use this postcode from now on.
    Set { postcode: String },
    /// Forget it.
    Clear,
}

#[derive(Subcommand, Debug)]
pub enum IslandAction {
    /// Which island is set now. The same as naming nothing at all.
    Show,
    /// north or south.
    Set { island: String },
    /// Forget it, and let Kmart decide.
    Clear,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_parser_is_well_formed() {
        Cli::command().debug_assert();
    }

    fn listing() -> Listing {
        Listing {
            limit: 20,
            page: 1,
            sort: None,
            brand: None,
            color: None,
            size: None,
            filters: Vec::new(),
            island: None,
            facets: false,
        }
    }

    #[test]
    fn no_flags_means_no_refinements() {
        assert!(listing().filters().unwrap().is_empty());
    }

    #[test]
    fn the_named_flags_become_the_facets_the_index_knows() {
        let l = Listing {
            brand: Some("Anko".into()),
            color: Some("Black".into()),
            ..listing()
        };
        assert_eq!(
            l.filters().unwrap(),
            [
                ("Brand".to_string(), "Anko".to_string()),
                ("Colour".to_string(), "Black".to_string())
            ]
        );
    }

    #[test]
    fn an_arbitrary_facet_can_be_given_because_they_differ_per_category() {
        let l = Listing {
            filters: vec![
                "Power Rating=1000 to 1500W".into(),
                "Capacity=0 - 1 L".into(),
            ],
            ..listing()
        };
        let filters = l.filters().unwrap();
        assert_eq!(filters[0].0, "Power Rating");
        assert_eq!(filters[0].1, "1000 to 1500W");
        assert_eq!(filters[1].0, "Capacity");
    }

    #[test]
    fn one_facet_can_be_given_twice_and_both_survive() {
        let l = Listing {
            filters: vec!["Colour=Black".into(), "Colour=White".into()],
            ..listing()
        };
        assert_eq!(l.filters().unwrap().len(), 2);
    }

    #[test]
    fn a_filter_that_is_not_a_pair_is_refused_with_the_text_that_was_typed() {
        let l = Listing {
            filters: vec!["justaname".into()],
            ..listing()
        };
        let e = l.filters().unwrap_err();
        assert!(e.contains("justaname"), "{e}");

        let l = Listing {
            filters: vec!["=value".into()],
            ..listing()
        };
        assert!(l.filters().unwrap_err().contains("no facet name"));
    }

    #[test]
    fn the_country_flag_is_global_so_it_can_follow_the_subcommand() {
        let cli = Cli::try_parse_from(["kmart", "search", "mop", "--country", "au"]).unwrap();
        assert_eq!(cli.country, Some(Country::Au));
    }
}
