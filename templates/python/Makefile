# __NAME__ -- python
PY := python3
VERSION ?= 0.0.0-dev
export PYTHONPATH := src

.PHONY: build run test fmt fmt-check lint clean check dist

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

# Release artifact: sdist + wheel. The version comes from pyproject.toml; the
# release workflow checks that it agrees with the tag before getting here.
# `build` is fetched automatically only under CI -- locally, say so instead of
# installing into someone's environment behind their back.
dist:
	@$(PY) -c "import build" 2>/dev/null || { \
	  if [ -n "$$CI" ]; then $(PY) -m pip install --quiet build; \
	  else echo "the 'build' package is required: pip install build" >&2; exit 1; fi; }
	$(PY) -m build --outdir release
	@echo "built release/"

clean:
	rm -rf .ruff_cache .pytest_cache dist release
	find . -name __pycache__ -type d -exec rm -rf {} +

check: fmt-check lint build test
