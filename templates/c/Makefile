# __NAME__ -- c
CC ?= cc
CFLAGS ?= -std=c11 -Wall -Wextra -Werror -O2
BIN := bin/__NAME__
TEST_BIN := bin/test_greet
SRC := src/greet.c src/main.c

.PHONY: build run test fmt fmt-check clean check

build: $(BIN)

$(BIN): $(SRC) src/greet.h
	@mkdir -p bin
	$(CC) $(CFLAGS) -o $@ $(SRC)

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

clean:
	rm -rf bin

check: fmt-check test
