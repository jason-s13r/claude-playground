//! `cart` -- what is in it, and changing it.

use cli_kit::emit;
use gsnz_core::{Cart, Change, Quantity};
use gsnz_ui::CartView;

use crate::app::App;
use crate::cli::{CartAction, Unit};
use crate::error::{AppError, AppResult};

pub async fn run(app: &App, action: CartAction) -> AppResult<()> {
    let handle = app.handle()?;
    let cart = match action {
        CartAction::List => handle.cart().await?,
        CartAction::Add {
            sku,
            quantity,
            unit,
        } => {
            // Add is relative, so what is there now decides what to send: both
            // APIs take an absolute quantity per line and neither has an
            // increment.
            let current = handle.cart().await?;
            let key = resolve(&current, &sku);
            let wanted = add(current.line(&key).map(|l| l.quantity), quantity, &unit)?;
            handle
                .cart_apply(&[Change {
                    key,
                    quantity: wanted,
                }])
                .await?
        }
        CartAction::Update {
            sku,
            quantity,
            unit,
        } => {
            let current = handle.cart().await?;
            let key = resolve(&current, &sku);
            handle
                .cart_apply(&[Change {
                    key,
                    quantity: quantity_of(quantity, &unit)?,
                }])
                .await?
        }
        CartAction::Remove { sku } => {
            let current = handle.cart().await?;
            let key = resolve(&current, &sku);
            // Zero is how both sites remove a line; there is no delete verb.
            handle
                .cart_apply(&[Change {
                    key,
                    quantity: Quantity::units(0),
                }])
                .await?
        }
        CartAction::Clear { force } => {
            if !force {
                return Err(AppError::usage(
                    "emptying the cart cannot be undone: pass --force to mean it",
                ));
            }
            handle.cart_clear().await?
        }
    };
    emit(&mut app.out(), &CartView(&cart))?;
    Ok(())
}

/// What the user typed, resolved to the key a mutation needs.
///
/// Woolworths prints a stock code and accepts only a variant key, and people
/// type the stock code. A line already in the cart knows its own key, so match
/// on that first and fall back to what was typed.
fn resolve(cart: &Cart, typed: &str) -> String {
    cart.line(typed)
        .map(|l| l.key.clone())
        .unwrap_or_else(|| typed.to_string())
}

fn quantity_of(value: f64, unit: &Unit) -> AppResult<Quantity> {
    if value < 0.0 {
        return Err(AppError::usage("a quantity cannot be negative"));
    }
    Ok(if unit.is_weight() {
        Quantity::kilograms(value)
    } else {
        Quantity::units(value.round() as u32)
    })
}

fn add(existing: Option<Quantity>, delta: f64, unit: &Unit) -> AppResult<Quantity> {
    let delta = quantity_of(delta, unit)?;
    Ok(match (existing, delta) {
        (Some(Quantity::Units { count }), Quantity::Units { count: more }) => {
            Quantity::units(count + more)
        }
        (Some(Quantity::Kilograms { kg }), Quantity::Kilograms { kg: more }) => {
            Quantity::kilograms(kg + more)
        }
        // A line's own unit wins over the flag: adding "1" to a line priced by
        // the kilogram means another kilogram, not one of something else.
        (Some(Quantity::Kilograms { kg }), Quantity::Units { count }) => {
            Quantity::kilograms(kg + count as f64)
        }
        (Some(Quantity::Units { count }), Quantity::Kilograms { kg }) => {
            Quantity::units(count + kg.round() as u32)
        }
        (None, delta) => delta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(unit: Option<&str>) -> Unit {
        Unit {
            kg: unit.map(str::to_string),
        }
    }

    #[test]
    fn adding_to_a_line_counts_in_that_lines_own_unit() {
        // `cart add 1` on a line sold by the kilogram means another kilogram.
        let got = add(Some(Quantity::kilograms(0.5)), 1.0, &units(None)).unwrap();
        assert_eq!(got, Quantity::kilograms(1.5));
    }

    #[test]
    fn adding_to_nothing_takes_the_flag_at_its_word() {
        assert_eq!(add(None, 2.0, &units(None)).unwrap(), Quantity::units(2));
        assert_eq!(
            add(None, 0.5, &units(Some("kg"))).unwrap(),
            Quantity::kilograms(0.5)
        );
    }

    #[test]
    fn a_negative_quantity_is_refused_rather_than_wrapped() {
        // `as u32` on a negative float saturates to zero, which would silently
        // delete the line instead of failing.
        assert!(quantity_of(-1.0, &units(None)).is_err());
    }
}
