//! `cart` -- what is in the basket, and changing it.
//!
//! Every change here is the two-step: read the product page, then spend the
//! token it carries. That is why `add` takes a product id and does two requests
//! -- there is no endpoint that adds to a cart from an id alone.

use cli_kit::emit;

use crate::app::App;
use crate::cli::CartAction;
use crate::error::AppResult;
use crate::views::CartView;

pub async fn run(app: &App, action: CartAction) -> AppResult<()> {
    let client = app.client()?;
    let (cart, note) = match action {
        CartAction::List => (client.cart().await?, None),
        CartAction::Add { pid, quantity } => {
            let pdp = client.pdp(&pid).await?;
            let cart = client.add_to_cart(&pdp, quantity).await?;
            (cart, Some(format!("Added {quantity} × {pid}.")))
        }
        CartAction::Set { pid, quantity } => {
            // Read first, so "no such line" is said plainly rather than
            // arriving as the site's own generic refusal -- and because a
            // removal needs the line's id, which only the cart knows.
            let line = find_line(&client.cart().await?, &pid)?;
            // Zero means remove. Not by setting the quantity to zero, which
            // the site accepts and then ignores, but by the removal proper.
            let cart = if quantity == 0 {
                client.remove_line(&line).await?
            } else {
                client.set_quantity(&line.id, quantity).await?
            };
            (
                cart,
                Some(if quantity == 0 {
                    format!("Removed {pid}.")
                } else {
                    format!("Set {pid} to {quantity}.")
                }),
            )
        }
        CartAction::Remove { pid } => {
            let line = find_line(&client.cart().await?, &pid)?;
            let cart = client.remove_line(&line).await?;
            (cart, Some(format!("Removed {pid}.")))
        }
    };

    // A write answers with a *partial* basket: `Cart-AddProduct` sends the
    // lines and neither a subtotal nor a count, so rendering its answer is a
    // table that quietly means something different from `cart list`. The site's
    // own page re-reads the minicart after a write for exactly this reason, so
    // this does too -- and keeps the write's answer if that read fails, because
    // the change has already landed and saying otherwise would be a lie.
    let cart = match note {
        Some(_) => client.cart().await.unwrap_or(cart),
        None => cart,
    };
    let view = match note {
        Some(note) => CartView::new(&cart).after(note),
        None => CartView::new(&cart),
    };
    emit(&mut app.out(), &view)?;
    Ok(())
}

/// The line a product id names.
///
/// By product id rather than by line id because that is what a person has in
/// hand from a listing -- the line id is an opaque uuid they never see.
fn find_line(cart: &twlnz_api::Cart, pid: &str) -> AppResult<twlnz_api::CartLine> {
    cart.lines
        .iter()
        .find(|l| l.id.eq_ignore_ascii_case(pid))
        .cloned()
        .ok_or_else(|| twlnz_api::Error::NoSuchProduct(format!("{pid} in the cart")).into())
}
