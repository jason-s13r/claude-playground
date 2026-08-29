# __NAME__ -- python
PY := python3
export PYTHONPATH := src

.PHONY: build run test fmt fmt-check lint clean check

build:
	@echo "nothing to build"

run:
	$(PY) -m __IDENT__ $(ARGS)

test:
	$(PY) -m unittest discover -s tests -v

fmt:
	@command -v ruff >/dev/null 2>&1 && ruff format . || echo "ruff not installed, skipping"

fmt-check:
	@command -v ruff >/dev/null 2>&1 && ruff format --check . || echo "ruff not installed, skipping"

lint:
	@command -v ruff >/dev/null 2>&1 && ruff check . || echo "ruff not installed, skipping"

clean:
	rm -rf .ruff_cache .pytest_cache
	find . -name __pycache__ -type d -exec rm -rf {} +

check: fmt-check lint test
