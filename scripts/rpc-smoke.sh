#!/usr/bin/env bash
# End-to-end smoke test for the Rust `gn-drive serve --stdio` (protocol v1).
# Temp config dir + a fake rclone. Never touches the user's GN Drive data,
# keychain, or launchd, and never starts an OAuth flow.
set -euo pipefail

cd "$(dirname "$0")/.."
root="$(pwd)"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/gnd-rpc-smoke.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

# Fake rclone: records nothing, never binds :53682, never opens a browser.
fake="$tmp/rclone"
cat >"$fake" <<'FAKE'
#!/bin/sh
# Refuse anything that would start a provider sign-in.
case " $* " in
  *" authorize "*|*" reconnect "*|*" config create "*|*" config reconnect "*)
    echo "smoke fake: refusing oauth/config-create" >&2
    exit 99
    ;;
esac
case " $* " in
  *" version "*) echo "rclone v1.99.0-smoke" ;;
esac
exit 0
FAKE
chmod +x "$fake"

out="$tmp/out.ndjson"
err="$tmp/stderr.log"
cfg="$tmp/config"
mkdir -p "$cfg"

fail() {
  echo "FAIL: $*" >&2
  echo "--- stdout ---" >&2
  cat "$out" >&2 || true
  echo "--- stderr ---" >&2
  cat "$err" >&2 || true
  exit 1
}

cargo build -p gn-drive-cli
bin="$root/target/debug/gn-drive"
[ -x "$bin" ] || fail "missing $bin"

printf '%s\n' \
  '{"id":1,"method":"ping"}' \
  '{"id":"auth","method":"auth.status"}' \
  '{"id":3,"method":"state.snapshot","params":{"sections":["auth"]}}' \
  '{"id":4,"method":"flows.list"}' \
  '{"id":5,"method":"remotes.list"}' \
  | HOME="$tmp/home" GN_DRIVE_CONFIG_DIR="$cfg" GN_DRIVE_PASSWORD= \
    "$bin" serve --stdio --config-dir "$cfg" --rclone "$fake" >"$out" 2>"$err"

grep -q '"event":"hello"' "$out" || fail "missing hello frame"
head -n 1 "$out" | grep -q '"event":"hello"' || fail "hello is not the first frame"

grep -q '"id":1' "$out" || fail "missing ping response"
grep -q '"pong":true' "$out" || fail "ping did not return pong"
grep -q '"id":"auth"' "$out" || fail "missing auth.status response"
grep -q '"id":3' "$out" || fail "missing state.snapshot response"
grep -q '"id":4' "$out" || fail "missing flows.list response"
grep -q '"id":5' "$out" || fail "missing remotes.list response"
grep -q '"flows":\[\]' "$out" || fail "flows.list did not return an empty array"
grep -q '"remotes":\[\]' "$out" || fail "remotes.list did not return an empty array"
grep -q '"event":"app:shutdown"' "$out" || fail "missing app:shutdown frame"
grep -q '"reason":"eof"' "$out" || fail "shutdown reason is not eof"

lines="$(wc -l < "$out" | tr -d ' ')"
[ "$lines" -eq 7 ] || fail "expected 7 frames (hello + 5 responses + shutdown), got $lines"

if grep -q '53682\|oauth\|sign.in\|Dropbox' "$err"; then
  fail "stderr looks like an auth flow"
fi

echo "PASS: hello, ping, auth.status, state.snapshot, flows.list, remotes.list, app:shutdown"
echo "PASS: stdout carries protocol frames only ($lines lines); logs went to stderr"
