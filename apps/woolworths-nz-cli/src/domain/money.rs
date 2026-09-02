//! Money. Carried in cents so comparisons stay exact, rendered as dollars on
//! the way out.

use serde::Serializer;

/// Serialise cents as a dollar amount, so `--json` reads like a price list.
pub(crate) fn as_dollars<S: Serializer>(cents: &Option<i64>, s: S) -> Result<S::Ok, S::Error> {
    match cents {
        Some(c) => s.serialize_f64(*c as f64 / 100.0),
        None => s.serialize_none(),
    }
}

pub fn dollars(cents: i64) -> String {
    // The sign belongs outside the currency symbol: -$2.00, not $-2.00.
    let sign = if cents < 0 { "-" } else { "" };
    let cents = cents.unsigned_abs();
    format!("{sign}${}.{:02}", cents / 100, cents % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_money_without_floating_point_noise() {
        assert_eq!(dollars(429), "$4.29");
        assert_eq!(dollars(1000), "$10.00");
        assert_eq!(dollars(5), "$0.05");
        assert_eq!(dollars(-200), "-$2.00", "the sign goes outside the symbol");
        assert_eq!(dollars(0), "$0.00");
    }
}
