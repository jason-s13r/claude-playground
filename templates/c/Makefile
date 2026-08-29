# __NAME__ -- c
CC ?= cc
CFLAGS ?= -std=c11 -Wall -Wextra -Werror -O2
VERSION ?= 0.0.0-dev
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
ARCH := $(shell uname -m)
BIN := bin/__NAME__
TEST_BIN := bin/test_greet
SRC := src/greet.c src/main.c
DIST := release/__NAME__-$(VERSION)-$(OS)-$(ARCH).tar.gz

.PHONY: build run test fmt fmt-check clean check dist

build: $(BIN)

$(BIN): $(SRC) src/greet.h
	@mkdir -p bin
	$(CC) $(CFLAGS) -DVERSION=\"$(VERSION)\" -o $@ $(SRC)

run: $(BIN)
	./$(BIN) $(ARGS)

$(TEST_BIN): src/greet.c src/greet.h tests/test_greet.c
	@mkdir -p bin
	$(CC) $(CFLAGS) -o $@ src/greet.c tests/test_greet.c

test: $(TEST_BIN)
	./$(TEST_BIN)

fmt:
	@command -v clang-format >/dev/null 2>&1 && clang-format -i src/*.c src/*.h tests/*.c || echo "clang-format not installed, skipping"

fmt-check:
	@command -v clang-format >/dev/null 2>&1 && clang-format --dry-run --Werror src/*.c src/*.h tests/*.c || echo "clang-format not installed, skipping"

# Release artifact. VERSION is supplied by the release workflow from the tag.
dist: build
	@mkdir -p release
	tar -czf $(DIST) -C bin __NAME__
	@echo "built $(DIST)"

clean:
	rm -rf bin dist release

check: fmt-check build test
