//! Where the API lives.
//!
//! Plain fields, not resolved from the environment: this crate takes values.
//! The caller decides whether an override exists, which is also how a test
//! points the whole flow at a mock server.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    /// The storefront, which mints the guest token and hosts the API.
    pub origin: String,
    /// Where the login flow is served, a different host in production.
    pub auth: String,
}

impl Default for Endpoints {
    fn default() -> Endpoints {
        Endpoints {
            origin: "https://www.woolworths.co.nz".into(),
            auth: "https://auth.woolworths.co.nz".into(),
        }
    }
}

impl Endpoints {
    pub fn with_origin(mut self, origin: impl Into<String>) -> Endpoints {
        self.origin = trim(origin.into());
        self
    }

    pub fn with_auth(mut self, auth: impl Into<String>) -> Endpoints {
        self.auth = trim(auth.into());
        self
    }

    pub fn graphql(&self) -> String {
        format!("{}/api/graphql", self.origin)
    }
}

fn trim(s: String) -> String {
    s.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_trim_a_trailing_slash() {
        let e = Endpoints::default().with_origin("http://127.0.0.1:9/");
        assert_eq!(e.graphql(), "http://127.0.0.1:9/api/graphql");
        assert_eq!(e.auth, "https://auth.woolworths.co.nz", "untouched");
    }
}
