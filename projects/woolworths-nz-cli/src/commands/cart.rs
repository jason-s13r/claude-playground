//! `wwnz cart` -- reading and changing the shopping cart.

use anyhow::{bail, Result};

use crate::app::App;
use crate::cli::CartCommand;
use crate::commands::io::print_json;
use crate::domain::cart::{format_quantity, variant_key, Cart, Change};
use crate::output;

pub async fn run(app: &App, cmd: &CartCommand) -> Result<()> {
    let client = app.account_client()?;

    let (cart, note) = match cmd {
        CartCommand::List => (client.cart().await?, None),

        CartCommand::Add {
            sku,
            quantity,
            unit,
        } => {
            let key = variant_key(sku, unit.as_deref());
            // "Add" is relative, and the API only sets absolute quantities, so
            // whatever is already on the line has to be read first.
            let current = client.cart().await?;
            let have = current.line(&key).map(|l| l.quantity).unwrap_or(0.0);
            let want = have + checked(quantity.unwrap_or(1.0))?;
            let cart = client
                .cart_set(&[Change {
                    variant_key: key.clone(),
                    quantity: want,
                }])
                .await?;
            (cart, Some(changed(&key, have, want)))
        }

        CartCommand::Update {
            sku,
            quantity,
            unit,
        } => {
            let key = variant_key(sku, unit.as_deref());
            let want = checked(*quantity)?;
            let current = client.cart().await?;
            let have = current.line(&key).map(|l| l.quantity).unwrap_or(0.0);
            let cart = client
                .cart_set(&[Change {
                    variant_key: key.clone(),
                    quantity: want,
                }])
                .await?;
            (cart, Some(changed(&key, have, want)))
        }

        CartCommand::Remove { sku } => {
            let key = variant_key(sku, None);
            let current = client.cart().await?;
            let Some(line) = current.line(&key) else {
                bail!("{key} is not in the cart");
            };
            let have = line.quantity;
            // Zero is how the API removes a line; there is no delete.
            let cart = client
                .cart_set(&[Change {
                    variant_key: key.clone(),
                    quantity: 0.0,
                }])
                .await?;
            (cart, Some(changed(&key, have, 0.0)))
        }

        CartCommand::Clear { force } => {
            if !*force {
                bail!("emptying the cart cannot be undone; pass --force to confirm");
            }
            let cart = client.cart_clear().await?;
            (cart, Some("cart emptied".to_string()))
        }
    };

    render(app, &cart, note.as_deref())
}

/// A quantity clap parsed but that the API would only refuse.
///
/// `f64` is what makes weighed lines expressible, and it also makes `-1` and
/// `NaN` parse. Refusing them here beats sending them.
fn checked(quantity: f64) -> Result<f64> {
    if !quantity.is_finite() || quantity < 0.0 {
        bail!("quantity must be a number of zero or more, not {quantity}");
    }
    Ok(quantity)
}

/// What a change did, in the terms the user asked in.
fn changed(key: &str, from: f64, to: f64) -> String {
    let (from_q, to_q) = (format_quantity(from), format_quantity(to));
    if from == 0.0 && to == 0.0 {
        format!("{key} was not in the cart")
    } else if to == 0.0 {
        format!("removed {key}")
    } else if from == 0.0 {
        format!("added {key} × {to_q}")
    } else if from == to {
        format!("{key} was already × {to_q}")
    } else {
        format!("{key} × {from_q} → × {to_q}")
    }
}

fn render(app: &App, cart: &Cart, note: Option<&str>) -> Result<()> {
    if app.json {
        let mut value = serde_json::to_value(cart)?;
        if let (Some(note), Some(obj)) = (note, value.as_object_mut()) {
            obj.insert("change".into(), serde_json::json!(note));
        }
        print_json(&value);
        return Ok(());
    }
    if let Some(note) = note {
        println!("{note}\n");
    }
    output::print_cart(cart);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_is_described_by_what_it_actually_did() {
        assert_eq!(changed("1-EA", 0.0, 2.0), "added 1-EA × 2");
        assert_eq!(changed("1-EA", 1.0, 3.0), "1-EA × 1 → × 3");
        assert_eq!(changed("1-EA", 2.0, 0.0), "removed 1-EA");
        assert_eq!(changed("1-EA", 2.0, 2.0), "1-EA was already × 2");
        // Removing something absent is not a change, and should not claim to
        // be one.
        assert_eq!(changed("1-EA", 0.0, 0.0), "1-EA was not in the cart");
        // A weighed line reads in kilograms, and must not be rounded to a
        // count on the way to the screen.
        assert_eq!(changed("1-KGM", 0.3, 0.5), "1-KGM × 0.3 → × 0.5");
    }

    #[test]
    fn a_quantity_clap_accepts_but_the_api_would_not_is_refused_here() {
        assert!(checked(0.0).is_ok());
        assert!(checked(0.3).is_ok());
        assert!(checked(-1.0).is_err());
        assert!(checked(f64::NAN).is_err());
        assert!(checked(f64::INFINITY).is_err());
    }
}
