//! `loyalty` -- progress toward the next reward.
//!
//! Per fascia and not shared: Briscoes Club and the Rebel Sport programme are
//! separate schemes with separate balances, which is the account half of the
//! same split that gives the two fascias separate sign-ins.

use cli_kit::emit;

use crate::app::App;
use crate::error::AppResult;
use crate::views::LoyaltyView;

pub async fn run(app: &App, refresh: bool) -> AppResult<()> {
    // `refresh` asks the loyalty service rather than the storefront's cache.
    // The storefront warns that a reward can take a day to appear either way,
    // so this is the difference between stale by minutes and stale by hours.
    let loyalty = app.client()?.loyalty(refresh).await?;
    let mut out = app.out();
    emit(&mut out, &LoyaltyView::new(&loyalty, app.banner))?;
    Ok(())
}
