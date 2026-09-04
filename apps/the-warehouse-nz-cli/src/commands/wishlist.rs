//! `wishlist` -- what is saved for later, and changing it.
//!
//! Every write here re-reads the page afterwards, and that is not laziness:
//! the wishlist's own controllers answer with `{"success":true}` and nothing
//! else -- no list, no count -- so there is no result to render except the one
//! a second request goes and fetches.
//!
//! Reading first is also what supplies the ids. The controllers address a row
//! by its `uuid`, which is not the product id and not something a person has;
//! `wishlist` is where it comes from.

use cli_kit::emit;

use crate::app::App;
use crate::cli::WishlistAction;
use crate::error::AppResult;
use crate::views::WishlistView;

pub async fn run(app: &App, action: Option<WishlistAction>) -> AppResult<()> {
    let client = app.client()?;
    let note = match action.unwrap_or(WishlistAction::List) {
        WishlistAction::List => None,
        WishlistAction::Add { pid } => {
            // The one write here that still needs a product page: adding is
            // token-guarded like the cart's, while the rest are not.
            let pdp = client.pdp(&pid).await?;
            client.add_to_wishlist(&pdp).await?;
            Some(format!("Saved {}.", named(&pdp.detail.product.name, &pid)))
        }
        WishlistAction::Remove { pid } => {
            let item = find(&client.wishlist().await?, &pid)?;
            client.remove_from_wishlist(&item).await?;
            Some(format!("Stopped saving {}.", named(&item.name, &pid)))
        }
        WishlistAction::Set { pid, quantity } => {
            let item = find(&client.wishlist().await?, &pid)?;
            // Zero means take it off the list. The site's own field refuses
            // zero rather than treating it as a removal, so this is the
            // binary's idiom rather than the site's -- the same one `cart set`
            // uses, for the same reason.
            match quantity {
                0 => {
                    client.remove_from_wishlist(&item).await?;
                    Some(format!("Stopped saving {}.", named(&item.name, &pid)))
                }
                _ => {
                    client.set_wishlist_quantity(&item, quantity).await?;
                    Some(format!("Set {} to {quantity}.", named(&item.name, &pid)))
                }
            }
        }
        WishlistAction::MoveToCart { pid, quantity } => {
            let item = find(&client.wishlist().await?, &pid)?;
            // The quantity that was saved, unless the caller said another. It
            // is the number the person put there, so it is the better default
            // than one.
            let quantity = quantity.unwrap_or(item.quantity);
            // The add first, so a failure between the two leaves the product
            // saved rather than in neither place.
            client.add_saved_to_cart(&item, quantity).await?;
            client.remove_from_wishlist(&item).await?;
            Some(format!(
                "Moved {quantity} × {} to the cart.",
                named(&item.name, &pid)
            ))
        }
    };

    let wishlist = client.wishlist().await?;
    let view = match note {
        Some(note) => WishlistView::new(&wishlist).after(note),
        None => WishlistView::new(&wishlist),
    };
    emit(&mut app.out(), &view)?;
    Ok(())
}

/// The saved row a product id names.
///
/// By product id because that is what a person has from a listing, and because
/// the row id is an opaque uuid they never see -- but the row is what the
/// writes take, so this is where one becomes the other.
fn find(wishlist: &twlnz_api::Wishlist, pid: &str) -> AppResult<twlnz_api::WishlistItem> {
    wishlist
        .items
        .iter()
        .find(|i| i.id.eq_ignore_ascii_case(pid))
        .cloned()
        .ok_or_else(|| {
            let detail = match wishlist.complete() {
                true => format!("{pid} on the wishlist"),
                // The list is paged, so "not saved" would be a guess about the
                // rows this never saw.
                false => format!("{pid} on the first page of the wishlist"),
            };
            twlnz_api::Error::NoSuchProduct(detail).into()
        })
}

/// The product's name, or its id when the site gave no name.
fn named<'a>(name: &'a str, pid: &'a str) -> &'a str {
    match name.is_empty() {
        true => pid,
        false => name,
    }
}
