//! `wishlist` -- saving something for later.
//!
//! Add only. Reading and removing both need the wishlist page, which is
//! account-scoped HTML that has not been captured, so offering the commands
//! would mean guessing at markup.

use cli_kit::{emit, Out, View};
use serde::Serialize;
use std::io::Write;

use crate::app::App;
use crate::cli::WishlistAction;
use crate::error::AppResult;

pub async fn run(app: &App, action: WishlistAction) -> AppResult<()> {
    let client = app.client()?;
    match action {
        WishlistAction::Add { pid } => {
            let pdp = client.pdp(&pid).await?;
            client.add_to_wishlist(&pdp).await?;
            emit(
                &mut app.out(),
                &Added {
                    pid: pid.clone(),
                    product: pdp.detail.product.name.clone(),
                },
            )?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct Added {
    pid: String,
    product: String,
}

impl View for Added {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        if self.product.is_empty() {
            writeln!(out, "Added {} to the wishlist.", self.pid)
        } else {
            writeln!(out, "Added {} to the wishlist.", self.product)
        }
    }
}
