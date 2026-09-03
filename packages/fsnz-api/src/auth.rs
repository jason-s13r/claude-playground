//! Reading what a Foodstuffs token says about itself.
//!
//! The Club Plus login chain that mints one lands here next; for now this is
//! the half that needs no network.

/// The banners named in a session's `linkedAccounts` claim.
///
/// Reported, not interpreted. It is tempting to read a missing banner as "that
/// banner will not work", and that is wrong: an account listing `MNW` alone
/// still mints a `PNS`-scoped token and reads its PAK'nSAVE cart back fine.
/// What the claim actually gates has not been established.
pub fn linked_banners(token: &str) -> Vec<String> {
    let Some(payload) = net_kit::jwt::claims(token) else {
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

    fn jwt(payload: serde_json::Value) -> String {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("header.{encoded}.signature")
    }

    #[test]
    fn reads_linked_accounts_as_an_array() {
        let token = jwt(serde_json::json!({ "linkedAccounts": [{ "banner": "MNW" }] }));
        assert_eq!(linked_banners(&token), vec!["MNW".to_string()]);
    }

    #[test]
    fn reads_linked_accounts_sent_as_a_json_string() {
        let token = jwt(serde_json::json!({ "linkedAccounts": "[{\"banner\":\"PNS\"}]" }));
        assert_eq!(linked_banners(&token), vec!["PNS".to_string()]);
    }

    #[test]
    fn anything_else_reads_as_nothing_rather_than_failing() {
        assert!(linked_banners("not-a-jwt").is_empty());
        assert!(linked_banners(&jwt(serde_json::json!({}))).is_empty());
        assert!(linked_banners(&jwt(serde_json::json!({ "linkedAccounts": 7 }))).is_empty());
    }
}
