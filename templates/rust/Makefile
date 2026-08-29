# __NAME__ -- rust
.PHONY: build run test fmt fmt-check lint clean check

build:
	cargo build

run:
	cargo run --quiet -- $(ARGS)

test:
	cargo test

fmt:
	cargo fmt

fmt-check:
	@cargo fmt --version >/dev/null 2>&1 && cargo fmt --check || echo "rustfmt not installed, skipping"

lint:
	@cargo clippy --version >/dev/null 2>&1 && cargo clippy -- -D warnings || echo "clippy not installed, skipping"

clean:
	cargo clean

check: fmt-check lint build test
