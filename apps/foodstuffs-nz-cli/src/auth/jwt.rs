//! Reading the claims out of the JWTs Foodstuffs issues.
//!
//! Nothing here trusts a claim or verifies a signature. It reads what the
//! issuer said, so expiry can be anticipated and scope can be reported.

use base64::Engine;

/// A JWT's payload, signature unverified.
pub fn claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(payload))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Read `exp` out of a JWT payload, in milliseconds.
pub fn expiry_ms(token: &str) -> Option<u64> {
    let exp = claims(token)?.get("exp")?.as_u64()?;
    Some(exp.saturating_mul(1000))
}

/// The `banner` claim on a token: `NAT`, `MNW` or `PNS`.
///
/// Worth reporting on its own, because a `NAT` token is not rejected by the
/// cart -- it authenticates and answers with an empty cart belonging to nobody.
pub fn banner_claim(token: &str) -> Option<String> {
    claims(token)?.get("banner")?.as_str().map(str::to_string)
}

/// The banners named in a session's `linkedAccounts` claim.
///
/// Reported, not interpreted. It is tempting to read a missing banner as "that
/// banner will not work", and that is wrong: an account listing `MNW` alone
/// still mints a `PNS`-scoped token and reads its PAK'nSAVE cart back fine.
/// What the claim actually gates has not been established.
pub fn linked_banners(token: &str) -> Vec<String> {
    let Some(payload) = claims(token) else {
        return Vec::new();
    };
    let Some(linked) = payload.get("linkedAccounts") else {
        return Vec::new();
    };
    // Seen both as an array and as a JSON string holding one.
    let parsed;
    let array = match linked {
        serde_json::Value::Array(a) => a,
        serde_json::Value::String(s) => match serde_json::from_str(s) {
            Ok(serde_json::Value::Array(a)) => {
                parsed = a;
                &parsed
            }
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    array
        .iter()
        .filter_map(|e| e.get("banner")?.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt_with_exp(exp_secs: u64) -> String {
        let payload = serde_json::json!({ "exp": exp_secs }).to_string();
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        format!("header.{encoded}.signature")
    }

    #[test]
    fn reads_expiry_out_of_a_jwt() {
        assert_eq!(
            expiry_ms(&jwt_with_exp(1_700_000_000)),
            Some(1_700_000_000_000)
        );
    }

    #[test]
    fn reads_the_banner_and_linked_accounts_claims() {
        let payload = serde_json::json!({
            "banner": "MNW",
            "linkedAccounts": [{ "banner": "MNW" }],
        })
        .to_string();
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        let token = format!("header.{encoded}.signature");
        assert_eq!(banner_claim(&token).as_deref(), Some("MNW"));
        assert_eq!(linked_banners(&token), vec!["MNW".to_string()]);
    }
}
