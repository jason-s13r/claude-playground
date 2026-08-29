# __NAME__ -- go
BIN := bin/__NAME__

.PHONY: build run test fmt fmt-check lint clean check

build:
	go build -o $(BIN) .

run:
	go run . $(ARGS)

test:
	go test ./...

fmt:
	gofmt -w .

fmt-check:
	@out=$$(gofmt -l .); if [ -n "$$out" ]; then echo "needs gofmt:"; echo "$$out"; exit 1; fi

lint:
	go vet ./...

clean:
	rm -rf bin

check: fmt-check lint test
