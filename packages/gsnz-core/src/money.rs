//! Money is `i64` cents everywhere inside the program and dollars on the way
//! out. Nothing here is a float: a price that has been through an `f64` is a
//! price that can print as `$4.289999999`.

use serde::Serializer;

/// Cents as a dollar string. The sign goes outside the symbol -- `-$2.00`, not
/// `$-2.00`, which is what a person reading a discount expects.
pub fn dollars(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let cents = cents.unsigned_abs();
    format!("{sign}${}.{:02}", cents / 100, cents % 100)
}

/// `serialize_with` for a required money field. JSON carries `4.29`, not `429`.
pub fn as_dollars<S: Serializer>(cents: &i64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(*cents as f64 / 100.0)
}

/// `serialize_with` for an optional money field. Woolworths leaves most money
/// optional, so this is the common case rather than the exception.
pub fn as_dollars_opt<S: Serializer>(cents: &Option<i64>, s: S) -> Result<S::Ok, S::Error> {
    match cents {
        Some(c) => as_dollars(c, s),
        None => s.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dollars_and_cents() {
        assert_eq!(dollars(429), "$4.29");
        assert_eq!(dollars(400), "$4.00");
        assert_eq!(dollars(5), "$0.05");
        assert_eq!(dollars(0), "$0.00");
    }

    #[test]
    fn puts_the_sign_outside_the_symbol() {
        assert_eq!(dollars(-200), "-$2.00");
        assert_eq!(dollars(-5), "-$0.05");
    }

    #[test]
    fn serialises_as_dollars_not_cents() {
        #[derive(serde::Serialize)]
        struct P {
            #[serde(serialize_with = "as_dollars")]
            price: i64,
            #[serde(serialize_with = "as_dollars_opt")]
            unit: Option<i64>,
        }
        let json = serde_json::to_string(&P {
            price: 429,
            unit: None,
        })
        .unwrap();
        assert_eq!(json, r#"{"price":4.29,"unit":null}"#);
    }
}
