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
    "WORKDIR /build/parish/apps/ui"
require_line \
    "copies compile-time inference configuration" \
    "COPY parish/config/ parish/config/"
require_line \
    "builds the parish-server package and binary" \
    "RUN cargo build --release -p parish-server --bin parish-server"
require_line \
    "makes the built UI visible to parish-server's CSP build script" \
    "COPY --from=frontend /build/parish/apps/ui/dist /build/parish/apps/ui/dist/"
require_line \
    "copies the built parish-server binary" \
    "COPY --from=builder /build/parish/target/release/parish-server ./parish-server"
require_line \
    "packages the repository-relative frontend output" \
    "COPY --from=frontend /build/parish/apps/ui/dist ./apps/ui/dist/"
require_line \
    "starts parish-server with explicit packaged paths" \
    'CMD ["sh", "-c", "exec ./parish-server --port ${PORT:-3001} --data-dir /app/mods/rundale --static-dir /app/apps/ui/dist"]'

reject_text "does not build the retired parish server package" "-p parish --bin parish"
reject_text "does not invoke the retired multiplexed web flag" "./parish --web"

if [[ "$failures" -ne 0 ]]; then
    echo "deploy-dockerfile.test.sh: $failures assertion(s) failed." >&2
    exit 1
fi

echo "deploy-dockerfile.test.sh: all assertions passed."
