# __NAME__ -- rust
VERSION ?= 0.0.0-dev
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
ARCH := $(shell uname -m)
DIST := release/__NAME__-$(VERSION)-$(OS)-$(ARCH).tar.gz

.PHONY: build run test fmt fmt-check lint clean check dist

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

# Release artifact. The binary's own version comes from Cargo.toml; the release
# workflow checks that it agrees with the tag before getting here.
dist:
	cargo build --release
	@mkdir -p release
	tar -czf $(DIST) -C target/release __NAME__
	@echo "built $(DIST)"

clean:
	cargo clean
	rm -rf dist release

check: fmt-check lint build test
