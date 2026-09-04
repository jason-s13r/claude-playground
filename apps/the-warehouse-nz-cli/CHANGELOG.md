# Changelog

## the-warehouse-nz-cli/v0.1.0 (2026-09-04)

### Features

- show and change the wishlist
  `twlnz wishlist` prints what is saved with no subcommand, because
  reading is what a wishlist is mostly for; `list` still works, so the
  habit is not punished. Then `add`, `remove`, `set` -- zero removes, as
  in `cart set` -- and `move-to-cart`.

  The site addresses a saved row by a uuid a person never sees, so every
  write reads the list first to turn a product id into the row, and reads
  it again afterwards because these controllers answer with a flag and no
  model.

  `move-to-cart` is two changes, add then remove, in that order: the site's
  own button is two requests, and a failure between them leaves the
  product in the cart and still saved rather than in neither. It defaults
  to the quantity saved against the row.

  One price column where `cart` has two: saving quotes no line total, and
  multiplying the unit price would invent one.

- add twlnz
  Search, browse, per-store stock, variations, cart and wishlist against
  The Warehouse, on cli-kit and twlnz-api.

  Not part of the gsnz family: different trade, no Warehouse adapter in
  gsnz, so it shares only the halves with no domain in them.

  Two settings the site itself conflates are separate commands here.
  `island` picks which half of the country listings are priced for;
  `region` picks one of the 16 NZ- codes a store search runs against.
  Store search needs neither -- the directory is cached to disk for a
  week and fetched four regions at a time, since a burst is the shape
  that gets a client throttled.

  A cart write answers with a partial basket -- lines, no subtotal, no
  count -- so `cart add` and `cart remove` re-read the minicart before
  printing, which is what the site's own page does.

### Dependencies

- twlnz-api: 0.0.0 -> 0.1.0
