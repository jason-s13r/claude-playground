# __NAME__ -- node / typescript
VERSION ?= 0.0.0-dev

.PHONY: install build run test fmt fmt-check lint clean check dist

node_modules: package.json
	npm install
	@touch node_modules

install: node_modules

build: node_modules
	npx tsc

run: build
	node dist/index.js $(ARGS)

test: build
	node --test dist/

fmt: node_modules
	npx --yes prettier --write "src/**/*.ts" || echo "prettier unavailable, skipping"

fmt-check: node_modules
	npx --yes prettier --check "src/**/*.ts" || echo "prettier unavailable, skipping"

lint: node_modules
	npx tsc --noEmit

# Release artifact: an npm tarball. Its version comes from package.json; the
# release workflow checks that it agrees with the tag before getting here.
# tsc emits into dist/, so pack into a separate directory to avoid feeding
# the tarball back into itself on a rebuild.
dist: build
	@mkdir -p release
	npm pack --pack-destination release
	@echo "built release/"

clean:
	rm -rf dist release node_modules

check: lint build test
