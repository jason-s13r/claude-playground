//! Credentials and loyalty.

use std::io::{self, Write};

use bgnz_api::Loyalty;
use cli_kit::{table, Out, View};
use serde::Serialize;

/// What is held for one fascia.
///
/// Two credentials, reported apart because they lapse apart and the fixes
/// differ: a missing storefront token is invisible and self-healing, a missing
/// sign-in needs a browser.
#[derive(Serialize)]
pub struct BannerAuth {
    pub banner: String,
    pub signed_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub token_fresh: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<String>,
    /// Why the credentials could not be read, when that is what happened.
    ///
    /// Told apart from "signed out" deliberately: a locked keychain, or one
    /// whose prompt was dismissed, would otherwise report the same thing as
    /// having never signed in -- and send someone to sign in again for a
    /// problem that is not about credentials at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
}

/// Both fascias at once, because "am I signed in" has two answers here and
/// showing only the current one is how someone concludes the tool is broken.
#[derive(Serialize)]
pub struct AuthStatus {
    pub banners: Vec<BannerAuth>,
}

impl View for AuthStatus {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        // A line per fascia rather than a table, which is the shape every
        // other tool here uses for this: two or three facts about each of two
        // things, where a table's rules cost more than they explain.
        for b in &self.banners {
            let mark = match (&b.unreadable, b.signed_in) {
                (Some(_), _) => out.warn("unknown"),
                (None, true) => out.good("signed in"),
                (None, false) => out.dim("signed out"),
            };
            match b.email.as_deref() {
                Some(who) => writeln!(out, "{}  {mark} {who}", b.banner)?,
                None => writeln!(out, "{}  {mark}", b.banner)?,
            }
            if let Some(why) = &b.unreadable {
                writeln!(out, "  {}", out.bad(why))?;
                continue;
            }
            if !b.signed_in {
                continue;
            }
            match (&b.expires_in, b.token_fresh) {
                (Some(left), true) => writeln!(out, "  token expires in {left}")?,
                // Not a fault, so it does not get a warning colour: the next
                // command mints one from the stored sign-in, silently.
                _ => writeln!(out, "  {}", out.dim("token will be minted"))?,
            }
            writeln!(
                out,
                "  {}",
                out.dim("renewable without a password or a browser")
            )?;
        }
        if self
            .banners
            .iter()
            .all(|b| !b.signed_in && b.unreadable.is_none())
        {
            writeln!(
                out,
                "{}",
                out.dim("separate accounts: sign in to each, `bgnz -b briscoes auth login`")
            )?;
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct LoyaltyView<'a> {
    pub loyalty: &'a Loyalty,
    pub banner: String,
}

impl<'a> LoyaltyView<'a> {
    pub fn new(loyalty: &'a Loyalty, banner: bgnz_api::Banner) -> LoyaltyView<'a> {
        LoyaltyView {
            loyalty,
            banner: banner.name().to_string(),
        }
    }
}

impl View for LoyaltyView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let l = self.loyalty;
        if !l.member {
            return writeln!(
                out,
                "This account is not in the {} loyalty programme{}.",
                self.banner,
                if l.eligible { " yet" } else { "" }
            );
        }
        if let Some(description) = &l.description {
            writeln!(out, "{}", out.heading(description))?;
        }
        match (l.current_spend, l.threshold, l.to_next_reward) {
            (Some(spend), Some(threshold), Some(togo)) => {
                writeln!(
                    out,
                    "${spend:.2} of ${threshold:.2} — ${togo:.2} to the next reward{}",
                    l.progress
                        .map(|p| format!(" ({p:.0}%)"))
                        .unwrap_or_default()
                )?;
            }
            (Some(spend), _, _) => writeln!(out, "${spend:.2} spent")?,
            _ => {}
        }
        if l.vouchers.is_empty() {
            return writeln!(out, "No vouchers yet.");
        }
        let mut t = table(&["Code", "Reward", "Valid until", "Applied"]);
        for v in &l.vouchers {
            t.add_row(vec![
                v.code.clone().unwrap_or_else(|| "—".into()),
                v.title.clone().unwrap_or_else(|| "—".into()),
                v.valid_until.clone().unwrap_or_else(|| "—".into()),
                if v.applied { "yes" } else { "no" }.to_string(),
            ]);
        }
        writeln!(out, "{t}")
    }
}
