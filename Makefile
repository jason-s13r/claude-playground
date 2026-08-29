# Root task runner for the playground monorepo.
#
# Every project under projects/ ships its own Makefile exposing the same small
# set of targets. This file just fans them out, so you never have to remember
# whether a given project is cargo, go, npm, or a bare gcc invocation.
#
#   make test              # test every project
#   make test P=my-tool    # test one project
#   make run  P=my-tool ARGS="--help"
#   make new TEMPLATE=rust NAME=my-tool
#
SHELL := /bin/bash

PROJECTS := $(notdir $(patsubst %/,%,$(dir $(wildcard projects/*/Makefile))))
TEMPLATES := $(notdir $(patsubst %/,%,$(dir $(wildcard templates/*/))))

# P selects a subset of projects; empty means all of them.
P ?=
ifeq ($(strip $(P)),)
SELECTED := $(PROJECTS)
else
SELECTED := $(P)
endif

FANOUT_TARGETS := build test lint fmt fmt-check clean check

.DEFAULT_GOAL := help
.PHONY: help list new run $(FANOUT_TARGETS)

help:
	@echo "playground monorepo"
	@echo
	@echo "usage: make <target> [P=<project>] [ARGS=\"...\"]"
	@echo
	@echo "targets:"
	@echo "  check       fmt-check + lint + test (what CI runs)"
	@echo "  build       build every project (or just P=<project>)"
	@echo "  test        run tests"
	@echo "  lint        run static analysis"
	@echo "  fmt         format sources in place"
	@echo "  fmt-check   verify formatting without writing"
	@echo "  run         run one project; requires P=<project>"
	@echo "  clean       remove build artifacts"
	@echo "  list        list projects and templates"
	@echo "  new         scaffold a project: make new TEMPLATE=rust NAME=my-tool"
	@echo
	@echo "projects:  $(if $(PROJECTS),$(PROJECTS),<none yet>)"
	@echo "templates: $(TEMPLATES)"

list:
	@echo "projects:"
	@for p in $(PROJECTS); do echo "  $$p"; done
	@if [ -z "$(PROJECTS)" ]; then echo "  <none yet>"; fi
	@echo "templates:"
	@for t in $(TEMPLATES); do echo "  $$t"; done

$(FANOUT_TARGETS):
	@scripts/fanout.sh $@ $(SELECTED)

run:
	@if [ -z "$(strip $(P))" ]; then \
	  echo "error: 'run' needs a project: make run P=<project> [ARGS=\"...\"]" >&2; \
	  echo "projects: $(if $(PROJECTS),$(PROJECTS),<none yet>)" >&2; \
	  exit 2; \
	  fi
	@$(MAKE) --no-print-directory -C projects/$(P) run ARGS="$(ARGS)"

new:
	@if [ -z "$(strip $(TEMPLATE))" ] || [ -z "$(strip $(NAME))" ]; then \
	  echo "usage: make new TEMPLATE=<template> NAME=<name>" >&2; \
	  echo "templates: $(TEMPLATES)" >&2; \
	  exit 2; \
	  fi
	@scripts/new-project.sh $(TEMPLATE) $(NAME)
