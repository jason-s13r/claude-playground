//! Reading claims out of a JWT.
//!
//! Nothing here verifies a signature or trusts a claim. It reads what the
//! issuer said, so an expiry can be anticipated rather than discovered by a
//! failed request, and a token's scope can be reported.

use base64::Engine;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A JWT's payload, signature unverified.
pub fn claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(payload))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// `exp`, in milliseconds. JWTs carry seconds; everything downstream here
/// works in milliseconds, so the conversion happens once.
pub fn expiry_ms(token: &str) -> Option<u64> {
    let exp = claims(token)?.get("exp")?.as_u64()?;
    Some(exp.saturating_mul(1000))
}

/// A named string claim.
pub fn claim_str(token: &str, name: &str) -> Option<String> {
    claims(token)?.get(name)?.as_str().map(str::to_string)
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Is a credential still good, allowing for `skew`?
///
/// The margin is the point: a token that expires during the request it is
/// about to authorise fails in a way that reads as "wrong credentials".
pub fn fresh(expires_at_ms: u64, skew: Duration) -> bool {
    expires_at_ms > now_ms().saturating_add(skew.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt(payload: serde_json::Value) -> String {
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("header.{encoded}.signature")
    }

    #[test]
    fn reads_expiry_in_milliseconds() {
        let token = jwt(serde_json::json!({ "exp": 1_700_000_000u64 }));
        assert_eq!(expiry_ms(&token), Some(1_700_000_000_000));
    }

    #[test]
    fn reads_a_named_claim() {
        let token = jwt(serde_json::json!({ "banner": "MNW", "sub": "u1" }));
        assert_eq!(claim_str(&token, "banner").as_deref(), Some("MNW"));
        assert_eq!(claim_str(&token, "absent"), None);
    }

    #[test]
    fn garbage_reads_as_no_claims_rather_than_panicking() {
        assert!(claims("not-a-jwt").is_none());
        assert!(claims("").is_none());
        assert!(claims("a.!!!not-base64!!!.c").is_none());
        assert!(expiry_ms("header.eyJ9.sig").is_none());
    }

    #[test]
    fn freshness_leaves_room_for_the_request_it_authorises() {
        let now = now_ms();
        assert!(fresh(now + 120_000, Duration::from_secs(60)));
        // Expires in 30s, but the margin is 60s: not fresh enough to use.
        assert!(!fresh(now + 30_000, Duration::from_secs(60)));
        assert!(!fresh(0, Duration::from_secs(0)));
    }
}
