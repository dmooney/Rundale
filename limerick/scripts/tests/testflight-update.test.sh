#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
script="$repo_root/limerick/scripts/testflight-update.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# A stand-in python3 records its arguments and the variables it received.
mkdir -p "$work/bin"
cat >"$work/bin/python3" <<'EOF'
#!/usr/bin/env bash
{
    echo "args=$*"
    env | grep -E '^(ASC_|OTHER_SECRET=)' | sort
} >"$RUN_LOG"
EOF
chmod +x "$work/bin/python3"
export PATH="$work/bin:$PATH" RUN_LOG="$work/run.log" RUNDALE_DOTENV="$work/env"
unset ASC_KEY_PATH ASC_KEY_ID ASC_ISSUER_ID OTHER_SECRET

fail() {
    echo "testflight-update test: $*" >&2
    exit 1
}

has() {
    grep -qxF "$1" "$RUN_LOG" || fail "expected '$1' in: $(tr '\n' ' ' <"$RUN_LOG")"
}

# Quoted values are unquoted, ~ expands, other names stay out, and a final
# line without a trailing newline is still read.
printf '%s\n' 'OTHER_SECRET=leak' 'ASC_KEY_PATH="~/keys/AuthKey_ABC.p8"' "ASC_KEY_ID='ABC'" >"$work/env"
printf '%s' 'ASC_ISSUER_ID=1111-2222' >>"$work/env"
bash "$script" --verify none
has "args=$repo_root/mobile/scripts/release.py testflight --verify none"
has "ASC_KEY_PATH=$HOME/keys/AuthKey_ABC.p8"
has "ASC_KEY_ID=ABC"
has "ASC_ISSUER_ID=1111-2222"
if grep -q '^OTHER_SECRET=' "$RUN_LOG"; then fail "exported a name outside the allowlist"; fi

# A value already in the environment wins over .env.
ASC_KEY_ID=FROMENV bash "$script"
has "ASC_KEY_ID=FROMENV"
has "ASC_ISSUER_ID=1111-2222"

# CRLF line endings do not leave a carriage return in the value.
printf 'ASC_KEY_ID=CRLF\r\n' >"$work/env"
bash "$script"
has "ASC_KEY_ID=CRLF"

# Without the file, nothing is set and release.py still runs.
rm "$work/env"
bash "$script"
has "args=$repo_root/mobile/scripts/release.py testflight"
if grep -q '^ASC_' "$RUN_LOG"; then fail "set ASC variables without a .env"; fi

echo "testflight-update test: ok"
