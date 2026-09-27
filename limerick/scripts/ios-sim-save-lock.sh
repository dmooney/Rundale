#!/usr/bin/env bash
# Run the save-lock tests on the iOS Simulator, inside an app's sandbox data
# container (issue #2039).
#
# Builds the limerick-persistence lock unit tests and the cross-process
# lifecycle tests (tests/save_lock_processes.rs) for aarch64-apple-ios-sim,
# then runs each test binary on a simulator with `xcrun simctl spawn`. The
# tests keep their saves under
#   <app data container>/Library/Application Support/Rundale/save-lock-test
# which is where the iOS app keeps its save.
#
# The script creates and boots its own simulator, then shuts it down and
# deletes it on exit. Set IOS_SIM_DEVICE=<udid> to use an existing simulator
# instead; it is booted if needed and shut down afterwards only if this script
# booted it.
#
# What the Simulator cannot show: real suspension by the OS, jetsam, and the
# device sandbox's signal policy. See docs/adr/026-ios-save-lock.md.
#
# Usage: bash limerick/scripts/ios-sim-save-lock.sh   (or: just ios-sim-save-lock)

set -euo pipefail

TARGET=aarch64-apple-ios-sim
DEVICE_TYPE=com.apple.CoreSimulator.SimDeviceType.iPhone-17
# Any installed app gives a real sandbox data container to write into.
CONTAINER_APP=com.apple.mobilesafari
PER_BINARY_TIMEOUT=180

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v xcrun >/dev/null || ! xcrun simctl help >/dev/null 2>&1; then
    echo "error: Xcode command line tools with simctl are required" >&2
    exit 2
fi

cd "$repo_root"
rustup target add "$TARGET" >/dev/null

created_device=""
booted_here=""
cleanup() {
    if [[ -n "$booted_here" ]]; then
        xcrun simctl shutdown "$booted_here" >/dev/null 2>&1 || true
    fi
    if [[ -n "$created_device" ]]; then
        xcrun simctl delete "$created_device" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

if [[ -n "${IOS_SIM_DEVICE:-}" ]]; then
    device="$IOS_SIM_DEVICE"
else
    runtime="$(xcrun simctl list runtimes available | grep -o 'com.apple.CoreSimulator.SimRuntime.iOS-[0-9-]*' | sort -V | tail -1)"
    if [[ -z "$runtime" ]]; then
        echo "error: no iOS Simulator runtime is installed" >&2
        exit 2
    fi
    device="$(xcrun simctl create "Rundale save lock" "$DEVICE_TYPE" "$runtime")"
    created_device="$device"
fi

if ! xcrun simctl list devices | grep "$device" | grep -q "(Booted)"; then
    xcrun simctl boot "$device"
    booted_here="$device"
fi
xcrun simctl bootstatus "$device" -b >/dev/null

echo "== Simulator"
xcrun simctl list devices | grep "$device" | sed 's/^ *//'
xcrun simctl getenv "$device" SIMULATOR_RUNTIME_VERSION | sed 's/^/iOS runtime: /'

container="$(xcrun simctl get_app_container "$device" "$CONTAINER_APP" data)"
save_root="$container/Library/Application Support/Rundale/save-lock-test"
rm -rf "$save_root"
mkdir -p "$save_root" "$container/tmp"
echo "app data container: $container"
echo "save root: $save_root"

echo
echo "== Build ($TARGET)"
build_log="$(mktemp)"
(cd limerick && cargo test -p limerick-persistence --target "$TARGET" --no-run \
    --message-format=json 2>/dev/null) >"$build_log"
binary_for() {
    python3 - "$build_log" "$1" <<'PY'
import json, sys
path, name = sys.argv[1], sys.argv[2]
for line in open(path):
    try:
        message = json.loads(line)
    except ValueError:
        continue
    if (message.get("reason") == "compiler-artifact"
            and message.get("executable")
            and message["target"]["name"] == name
            and message["profile"]["test"]):
        print(message["executable"])
PY
}
unit_bin="$(binary_for limerick_persistence)"
process_bin="$(binary_for save_lock_processes)"
rm -f "$build_log"
if [[ -z "$unit_bin" || -z "$process_bin" ]]; then
    echo "error: build for $TARGET failed; run the cargo command above for details" >&2
    exit 1
fi
for binary in "$unit_bin" "$process_bin"; do
    echo "$(basename "$binary"): $(xcrun vtool -show-build "$binary" | awk '/platform/ {print "platform " $2}' | head -1)"
done

run_on_simulator() {
    local binary="$1"
    shift
    # simctl spawn passes SIMCTL_CHILD_* variables to the spawned process.
    SIMCTL_CHILD_LIMERICK_LOCK_TEST_ROOT="$save_root" \
        SIMCTL_CHILD_TMPDIR="$container/tmp" \
        xcrun simctl spawn "$device" "$binary" "$@" &
    local pid=$!
    (sleep "$PER_BINARY_TIMEOUT" && kill -9 "$pid" 2>/dev/null) &
    local watchdog=$!
    disown "$watchdog"
    local status=0
    wait "$pid" || status=$?
    pkill -P "$watchdog" 2>/dev/null || true
    kill "$watchdog" 2>/dev/null || true
    return "$status"
}

status=0
echo
echo "== Lock unit tests on the Simulator"
run_on_simulator "$unit_bin" lock:: --test-threads=1 || status=1

echo
echo "== Cross-process lifecycle tests on the Simulator"
run_on_simulator "$process_bin" --test-threads=1 --nocapture || status=1

echo
if [[ "$status" -eq 0 ]]; then
    echo "iOS Simulator save-lock tests: PASS"
else
    echo "iOS Simulator save-lock tests: FAIL"
fi
exit "$status"
