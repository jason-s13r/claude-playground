# foodstuffs-nz-cli -- rust
VERSION ?= 0.0.0-dev
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
ARCH := $(shell uname -m)
BIN := fsnz
DIST := release/foodstuffs-nz-cli-$(VERSION)-$(OS)-$(ARCH).tar.gz

.PHONY: build run test fmt fmt-check lint clean check dist release-platforms

build:
	cargo build

run:
	cargo run --quiet -- $(ARGS)

test:
	cargo test

fmt:
	cargo fmt

# `A && B || C` would report a real failure in B as "not installed" and exit 0,
# which quietly hollows out `check`. Probe, then run.
fmt-check:
	@if cargo fmt --version >/dev/null 2>&1; then cargo fmt --check; \
	else echo "rustfmt not installed, skipping"; fi

lint:
	@if cargo clippy --version >/dev/null 2>&1; then cargo clippy --all-targets -- -D warnings; \
	else echo "clippy not installed, skipping"; fi

# Runners the release workflow builds this on, one per line. `fsnz update`
# only finds a binary for the machine asking, so every platform worth updating
# on has to be built here. A project that omits this target gets ubuntu-latest
# alone. Add macos-13 for Intel Macs; there is no cross-compiling, so each
# platform needs its own runner.
release-platforms:
	@echo ubuntu-latest
	@echo macos-14

# Release artifact. The binary's own version comes from Cargo.toml; the release
# workflow checks that it agrees with the tag before getting here.
dist:
	cargo build --release
	@mkdir -p release
	tar -czf $(DIST) -C target/release $(BIN)
	@echo "built $(DIST)"

clean:
	cargo clean
	rm -rf dist release

check: fmt-check lint build test
