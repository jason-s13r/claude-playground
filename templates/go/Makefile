# __NAME__ -- go
VERSION ?= 0.0.0-dev
GOOS_ := $(shell go env GOOS)
GOARCH_ := $(shell go env GOARCH)
BIN := bin/__NAME__
DIST := release/__NAME__-$(VERSION)-$(GOOS_)-$(GOARCH_).tar.gz

.PHONY: build run test fmt fmt-check lint clean check dist

build:
	go build -ldflags "-X main.version=$(VERSION)" -o $(BIN) .

run:
	go run -ldflags "-X main.version=$(VERSION)" . $(ARGS)

test:
	go test ./...

fmt:
	gofmt -w .

fmt-check:
	@out=$$(gofmt -l .); if [ -n "$$out" ]; then echo "needs gofmt:"; echo "$$out"; exit 1; fi

lint:
	go vet ./...

# Release artifact. VERSION is supplied by the release workflow from the tag.
dist: build
	@mkdir -p release
	tar -czf $(DIST) -C bin __NAME__
	@echo "built $(DIST)"

clean:
	rm -rf bin dist release

check: fmt-check lint build test
