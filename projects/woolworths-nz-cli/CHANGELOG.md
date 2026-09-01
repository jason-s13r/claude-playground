# Changelog

## woolworths-nz-cli/v0.1.0 (2026-09-01)

### Features

- add wwnz, a CLI for the Woolworths NZ API
  Search, specials, department browsing, store selection, cart and order
  history, against the GraphQL endpoint the woolworths.co.nz site calls.
  Replaces woolies-nz-cli, which broke when they moved to it.

  Two things about the API shape the tool. The store is a property of the
  cart rather than a search parameter, so selecting one is a mutation and
  every product command binds it before searching. Search, browse, specials
  and buy-again are four fields of one CompositeSearchInput, so they share a
  single document and a single SearchBy enum.

  Uses wreq rather than reqwest. Akamai sits in front of the site and scores
  the TLS handshake, not the headers: with rustls the storefront withholds
  its bot-manager cookies and the login is refused with a bare 400, while a
  browser handshake is answered normally. Matching headers by hand changes
  nothing, and curl -- which is how foodstuffs-nz-cli gets past Cloudflare --
  fares worse here than rustls.
