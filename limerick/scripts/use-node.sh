#!/usr/bin/env bash
# Sourced by gates and recipes that run Node: when fnm manages Node, put the
# version the repository's .nvmrc names on PATH. `fnm env` alone selects fnm's
# default, which can be an older major than package.json `engines` requires,
# so `npm ci` then fails with EBADENGINE. A Node already on PATH without fnm
# is left as is; ensure-deps.sh checks the major version either way.
if command -v fnm >/dev/null 2>&1; then
    eval "$(fnm env)"
    __rundale_nvmrc="$(git rev-parse --show-toplevel 2>/dev/null || pwd)/.nvmrc"
    if [ -f "$__rundale_nvmrc" ]; then
        fnm use --silent-if-unchanged "$(tr -d '[:space:]' <"$__rundale_nvmrc")" >/dev/null 2>&1 || true
    fi
    unset __rundale_nvmrc
fi
