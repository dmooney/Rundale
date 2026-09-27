#!/usr/bin/env bash
#
# Fast regression sensor for the production container entry point (#1709).
# Keeps the Cargo package, copied binary, and packaged runtime paths aligned
# without paying for a Docker build on every pull request.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
dockerfile="$repo_root/deploy/Dockerfile"
failures=0

require_line() {
    local description="$1"
    local expected="$2"
    if grep -Fqx -- "$expected" "$dockerfile"; then
        echo "ok   - $description"
    else
        echo "FAIL - $description: missing exact line: $expected" >&2
        failures=$((failures + 1))
    fi
}

reject_text() {
    local description="$1"
    local rejected="$2"
    if grep -Fq -- "$rejected" "$dockerfile"; then
        echo "FAIL - $description: found retired text: $rejected" >&2
        failures=$((failures + 1))
    else
        echo "ok   - $description"
    fi
}

require_line \
    "preserves repository-relative UI paths for provenance checks" \
    "WORKDIR /build/limerick/apps/ui"
require_line \
    "copies compile-time inference configuration" \
    "COPY limerick/config/ limerick/config/"
require_line \
    "builds the limerick-server package and binary" \
    "RUN cargo build --release -p limerick-server --bin limerick-server"
require_line \
    "makes the built UI visible to limerick-server's CSP build script" \
    "COPY --from=frontend /build/limerick/apps/ui/dist /build/limerick/apps/ui/dist/"
require_line \
    "copies the built limerick-server binary" \
    "COPY --from=builder /build/limerick/target/release/limerick-server ./limerick-server"
require_line \
    "packages the repository-relative frontend output" \
    "COPY --from=frontend /build/limerick/apps/ui/dist ./apps/ui/dist/"
require_line \
    "starts limerick-server with explicit packaged paths" \
    'CMD ["sh", "-c", "exec ./limerick-server --port ${PORT:-3001} --data-dir /app/mods/rundale --static-dir /app/apps/ui/dist"]'

reject_text "does not build the retired limerick server package" "-p limerick-engine --bin limerick-engine"
reject_text "does not invoke the retired multiplexed web flag" "./limerick --web"

if [[ "$failures" -ne 0 ]]; then
    echo "deploy-dockerfile.test.sh: $failures assertion(s) failed." >&2
    exit 1
fi

echo "deploy-dockerfile.test.sh: all assertions passed."
