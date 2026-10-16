#!/bin/bash
# The T13 drill as one unattended command (Git Bash on Windows): a real mission through the coordinator, the real canary
# (R136 waiver, harness-built candidate only), promotion, the 300 s probation, the rollback drill and the positive Resume.
# It starts hidden desktops and the candidate app, so run it while nobody plays. Evidence lands in $OUT; the exit code
# is 0 only when the candidate is healthy, running the landed commit, and the rollback drill rolled back.
#
#   bash testkit/scripts/t13-drill.sh <run-name> [receipts-dir]
#
# Needs the maintainer's Rust under E:\Apps\rust, the installed Claude Code and Codex signed in, and qualification
# receipts younger than 24 hours (copied from <receipts-dir>, default: $FABER_T13_RECEIPTS).
set -u
NAME=${1:?run name}
REPO=$(cd "$(dirname "$0")/../.." && pwd)
export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo' PATH="/e/Apps/rust/cargo/bin:$PATH"
T=${FABER_T13_TARGET:-/e/Documentos/ProjetosIA/faber/target}
export CARGO_TARGET_DIR="$(cygpath -w "$T/t13")"
win() { cygpath -w "$1"; }
STAGE=$T/t13-stage; B=$T/t13-baseline-src; OUT=$REPO/target/t13-probe; mkdir -p "$STAGE" "$OUT"
[ -n "${2:-}" ] && export FABER_T13_RECEIPTS=$2
: "${FABER_T13_RECEIPTS:?receipts dir}"
BRANCH=$(git -C "$REPO" rev-parse --abbrev-ref HEAD)

# 1. The baseline: a clone of the committed head carrying the staged regression and the harness feature.
rm -rf "$B"
git clone --quiet -c core.autocrlf=false --no-hardlinks --single-branch --branch "$BRANCH" "$REPO" "$B" || exit 1
cd "$B" || exit 1
git config user.name "Faber T13"; git config user.email t13@example.invalid
sed -i 's/(total >= b.total).then_some(total - b.total)/(total >= b.total).then_some(total + b.total)/' engine/src/usage.rs
grep -q 'then_some(total + b.total)' engine/src/usage.rs || { echo "regression not staged"; exit 1; }
sed -i 's|faber-engine = { path = "../engine" }|faber-engine = { path = "../engine", features = ["r132-harness"] }|' src-tauri/Cargo.toml
grep -q r132-harness src-tauri/Cargo.toml || { echo "feature not staged"; exit 1; }
git add -A && git commit --no-gpg-sign -q -m "stage a regression and the harness feature for the T13 drill" || exit 1
cp -r "$REPO/node_modules" node_modules; cp -r "$REPO/dist" dist
cargo build --release --package faber --bin faber --bin faber-local --offline --locked 2>&1 | tail -1
cp "$CARGO_TARGET_DIR/release/faber.exe" "$STAGE/faber.exe"
cp "$CARGO_TARGET_DIR/release/faber-local.exe" "$STAGE/faber-local-release.exe"

# 2. The harness and the recovery helper.
cd "$REPO" || exit 1
cargo build -p faber-testkit --examples 2>&1 | tail -1
cargo build -p faber --features test-hooks --bin faber-local 2>&1 | tail -1
cp "$CARGO_TARGET_DIR/debug/faber-local.exe" "$STAGE/faber-local-hooks.exe"
export FABER_T13_DEVROOT=$(win "$T/t13-dev") FABER_T13_APP=$(win "$STAGE/faber.exe")
export FABER_T13_HELPER=$(win "$STAGE/faber-local-release.exe") FABER_T13_DRILL_HELPER=$(win "$STAGE/faber-local-hooks.exe")
export FABER_T13_BASE_COMMIT=$(git -C "$B" rev-parse HEAD) FABER_T13_BASE_TREE=$(git -C "$B" rev-parse 'HEAD^{tree}')
export FABER_T13_SOURCE=$(win "$B") FABER_T13_BRANCH=$BRANCH FABER_T13_PROOF_TARGET=$(win "$T/t13") FABER_T13_REGRESSED=1
HARNESS=$CARGO_TARGET_DIR/debug/examples/project-qualification.exe

# 3. The mission (a Codex window that rolls its reset can fail the first calibration by one second: retry with a fresh root).
for attempt in 1 2 3 4 5; do
  rm -rf "$T/t13-dev"
  "$HARNESS" --live-seam "$(win "$OUT/$NAME-seam$attempt")" > "$OUT/$NAME-seam$attempt.out" 2>&1
  grep -q "caps: reason.budget.calibration" "$OUT/$NAME-seam$attempt.out" || break
  sleep 3
done
REPORT="$OUT/$NAME-seam$attempt/t13-mission.json"
grep -q '"step": "landed"' "$REPORT" 2>/dev/null || { echo "the mission did not land: see $OUT/$NAME-seam$attempt.out"; exit 2; }

# 4. Canary, promotion, probation, rollback drill.
export FABER_T13_MISSION_REPORT=$(win "$REPORT")
mkdir -p "$OUT/$NAME-promotion"
"$HARNESS" --live-promote "$(win "$OUT/$NAME-promotion")" > "$OUT/$NAME-promotion.out" 2>&1
code=$?
cat "$OUT/$NAME-promotion.out"
echo "evidence: $OUT/$NAME-promotion/t13-promotion.json (exit $code)"
exit $code
