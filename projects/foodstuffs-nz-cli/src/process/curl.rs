//! HTTP for the hosts that inspect the client.
//!
//! Foodstuffs put Cloudflare bot management in front of the storefronts and the
//! Club Plus API, and it fingerprints the connection rather than the headers.
//! Two things get rejected: HTTP/2 from anyone, and the TLS handshakes of both
//! rustls and macOS SecureTransport, which is every TLS backend `reqwest` can
//! be built with. OpenSSL and LibreSSL handshakes are accepted -- which is to
//! say `curl` is accepted -- so this shells out to it for those few requests.
//!
//! Everything else in this tool uses `reqwest` normally. This covers the
//! storefront token mint and the Club Plus login, and nothing else.
//!
//! Request bodies go in over stdin, never the command line: the login body
//! contains a password and arguments are visible to anyone running `ps`.

use anyhow::{bail, Context, Result};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// All values for a header name, compared case-insensitively.
    pub fn header_values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        self.headers
            .iter()
            .filter(move |(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub async fn request(
    method: &str,
    url: &str,
    headers: &[(&str, String)],
    body: Option<&str>,
) -> Result<Response> {
    let mut args: Vec<String> = vec![
        "--silent".into(),
        "--show-error".into(),
        // The whole point: HTTP/2 is fingerprinted and rejected.
        "--http1.1".into(),
        "--include".into(),
        "--max-time".into(),
        "30".into(),
        "--request".into(),
        method.to_string(),
    ];
    for (name, value) in headers {
        args.push("--header".into());
        args.push(format!("{name}: {value}"));
    }
    if body.is_some() {
        // @- reads the body from stdin, keeping it out of the argument list.
        args.push("--data-binary".into());
        args.push("@-".into());
    }
    args.push(url.to_string());

    let mut child = tokio::process::Command::new("curl")
        .args(&args)
        .stdin(if body.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running curl (it is needed for the hosts that reject other clients)")?;

    if let Some(body) = body {
        let mut stdin = child.stdin.take().context("curl stdin")?;
        stdin
            .write_all(body.as_bytes())
            .await
            .context("writing the request body to curl")?;
        stdin.shutdown().await.ok();
    }

    let out = child.wait_with_output().await.context("waiting for curl")?;
    if !out.status.success() {
        bail!(
            "curl failed for {url}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    parse(&String::from_utf8_lossy(&out.stdout), url)
}

/// Split `curl --include` output into the final header block and the body.
fn parse(raw: &str, url: &str) -> Result<Response> {
    // A 1xx or redirect leaves earlier header blocks in front; the last one wins.
    let mut rest = raw;
    let mut status = None;
    let mut headers = Vec::new();

    loop {
        let Some(split) = find_blank_line(rest) else {
            break;
        };
        let (block, after) = rest.split_at(split.0);
        let mut lines = block.lines();
        let Some(first) = lines.next() else { break };
        if !first.starts_with("HTTP/") {
            break;
        }
        status = first
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse::<u16>().ok());
        headers = lines
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        rest = &after[split.1..];
        // Informational and redirect responses are followed by another block.
        if !matches!(status, Some(100..=199) | Some(300..=399)) {
            break;
        }
    }

    Ok(Response {
        status: status.with_context(|| format!("no HTTP status in curl's output for {url}"))?,
        headers,
        body: rest.to_string(),
    })
}

/// Offset of the blank line ending a header block, and its length.
fn find_blank_line(s: &str) -> Option<(usize, usize)> {
    if let Some(i) = s.find("\r\n\r\n") {
        return Some((i, 4));
    }
    s.find("\n\n").map(|i| (i, 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_plain_response() {
        let raw = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: a=1\r\n\r\n{\"ok\":true}";
        let r = parse(raw, "u").unwrap();
        assert_eq!(r.status, 200);
        assert!(r.is_success());
        assert_eq!(r.body, "{\"ok\":true}");
        assert_eq!(r.header_values("set-cookie").collect::<Vec<_>>(), ["a=1"]);
    }

    #[test]
    fn keeps_the_last_block_after_a_redirect() {
        let raw = "HTTP/1.1 302 Found\r\nLocation: /next\r\n\r\nHTTP/1.1 200 OK\r\nSet-Cookie: fs-user-token=abc\r\n\r\nbody here";
        let r = parse(raw, "u").unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body, "body here");
        assert_eq!(
            r.header_values("Set-Cookie").collect::<Vec<_>>(),
            ["fs-user-token=abc"]
        );
    }

    #[test]
    fn header_lookup_ignores_case_and_finds_repeats() {
        let raw = "HTTP/1.1 200 OK\r\nset-cookie: a=1\r\nSet-Cookie: b=2\r\n\r\n";
        let r = parse(raw, "u").unwrap();
        assert_eq!(
            r.header_values("SET-COOKIE").collect::<Vec<_>>(),
            ["a=1", "b=2"]
        );
    }

    #[test]
    fn a_response_with_no_status_line_is_an_error() {
        assert!(parse("garbage without headers", "u").is_err());
    }

    #[test]
    fn error_statuses_are_not_success() {
        let r = parse("HTTP/1.1 403 Forbidden\r\n\r\nnope", "u").unwrap();
        assert!(!r.is_success());
        assert_eq!(r.body, "nope");
    }
}
