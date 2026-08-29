# __NAME__ -- node / typescript
.PHONY: install build run test fmt fmt-check lint clean check

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

clean:
	rm -rf dist node_modules

check: lint test
