#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Copyright (c) lapoule.dev
#
# Runs the test suite on a remote BuildKit builder, not on this machine, as
# one image build whose output is the logs (docker/Dockerfile.test).
#
#   scripts/test-on-builder.sh                        # every step: fmt test clippy wasm
#   scripts/test-on-builder.sh -p voxelle --test model  # `cargo test` with these arguments
#   VOXELLE_TEST_STEPS="clippy wasm" scripts/test-on-builder.sh
#   VOXELLE_TEST_REF=origin/my-branch VOXELLE_TEST_COMMANDS=cmds.txt scripts/test-on-builder.sh
#
# Arguments are `cargo test`'s; with arguments only the `test` step runs,
# unless VOXELLE_TEST_STEPS says otherwise.
#
# VOXELLE_TEST_COMMANDS names a file of shell commands, one per line (blank
# lines and # comments skipped): each runs as a step of its own, cmd-1,
# cmd-2…, from the workspace root, after the named steps; with it and no
# VOXELLE_TEST_STEPS, only the commands run. Changes the steps make to the
# tree come back in changes.patch.
#
# What is tested: the working tree's tracked files (committed or not), or
# VOXELLE_TEST_REF=<commit or branch>. The logs land in
# target/test-on-builder/<time>/: <step>.log and <step>.exit per step,
# summary.txt (ending with `=== tested <sha> (<ref>)`), changes.patch,
# exit-code and build.log. The script exits with the first failing step's code.
#
#   VOXELLE_BUILDER (or TUILE_BUILDER)  buildx builder to run on      (always)
#   VOXELLE_TEST_SLOT (or TUILE_TEST_SLOT)  the target directory of the
#                       builder used by this run (default 0)
#   VOXELLE_CARGO_JOBS  CARGO_BUILD_JOBS on the builder (default 4)
#   VOXELLE_TEST_THREADS  RUST_TEST_THREADS (default 4)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

case "${1:-}" in -h|--help) sed -n '5,33p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;; esac
BUILDER="${VOXELLE_BUILDER:-${TUILE_BUILDER:-}}"
if [ -z "$BUILDER" ]; then
    echo "error: VOXELLE_BUILDER (or TUILE_BUILDER) is not set — the buildx builder to run on" >&2
    exit 2
fi

COMMANDS="${VOXELLE_TEST_COMMANDS:-}"
if [ -n "$COMMANDS" ] && [ ! -f "$COMMANDS" ]; then
    echo "error: VOXELLE_TEST_COMMANDS=${COMMANDS} is not a file" >&2
    exit 2
fi
if [ -n "$COMMANDS" ]; then
    STEPS="${VOXELLE_TEST_STEPS:-}"
    TEST_ARGS="$(printf '%q ' "${@:---workspace}")"
elif [ "$#" -gt 0 ]; then
    STEPS="${VOXELLE_TEST_STEPS:-test}"
    TEST_ARGS="$(printf '%q ' "$@")"
else
    STEPS="${VOXELLE_TEST_STEPS:-fmt test clippy wasm}"
    TEST_ARGS="--workspace"
fi

# A commit of the working tree as it is (tracked files), without touching it.
REF="${VOXELLE_TEST_REF:-$(git -C "$REPO_ROOT" stash create 2>/dev/null || true)}"
REF="${REF:-HEAD}"
COMMIT="$(git -C "$REPO_ROOT" rev-parse --short "$REF")"

OUT="${REPO_ROOT}/target/test-on-builder/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$OUT"
CTX="${OUT}/context.tar"
git -C "$REPO_ROOT" archive --format=tar -o "$CTX" "$REF"
# The runner travels beside the tree, from THIS checkout (an older commit
# under test may have none), and so do the commands; the image removes
# .test-on-builder/ before the steps run.
EXTRA="${OUT}/ctx-extra/.test-on-builder"
mkdir -p "$EXTRA"
cp "${REPO_ROOT}/docker/Dockerfile.test" "$EXTRA/"
if [ -n "$COMMANDS" ]; then cp "$COMMANDS" "${EXTRA}/commands"; fi
tar -rf "$CTX" -C "${OUT}/ctx-extra" .test-on-builder
rm -rf "${OUT}/ctx-extra"

echo "=== ${STEPS}${COMMANDS:+ + commands of ${COMMANDS}} (cargo test ${TEST_ARGS}) on ${BUILDER} (${COMMIT}) → ${OUT}"
set +o pipefail
docker buildx build --builder "$BUILDER" --platform linux/amd64 --progress plain \
    -f .test-on-builder/Dockerfile.test \
    --build-arg "CARGO_TEST_ARGS=${TEST_ARGS}" \
    --build-arg "TEST_STEPS=${STEPS}" \
    --build-arg "CARGO_JOBS=${VOXELLE_CARGO_JOBS:-4}" \
    --build-arg "TEST_THREADS=${VOXELLE_TEST_THREADS:-4}" \
    --build-arg "TARGET_SLOT=${VOXELLE_TEST_SLOT:-${TUILE_TEST_SLOT:-0}}" \
    --output "type=local,dest=${OUT}" \
    - < "$CTX" 2>&1 | tee "${OUT}/build.log" \
    | { grep --line-buffered -E '^#[0-9]+ [0-9.]+ (=== [a-z0-9-]+: |\[[a-z0-9-]+\] )' || true; }
build_rc="${PIPESTATUS[0]}"
set -o pipefail
rm -f "$CTX"

if [ "$build_rc" -ne 0 ] || [ ! -f "${OUT}/exit-code" ]; then
    echo "test-on-builder: the build itself failed (${OUT}/build.log)" >&2
    grep -E 'ERROR|error:' "${OUT}/build.log" | grep -v '^#[0-9]* [0-9.]* |' | sed -n 1,20p >&2 || true
    # What the steps printed before the cut: each step's lines from the
    # progress (`|name| line`).
    sed -nE 's/^#[0-9]+ [0-9.]+ === ([a-z0-9-]+): start .*/\1/p' "${OUT}/build.log" | sort -u \
        | while IFS= read -r name; do
            sed -nE "s/^#[0-9]+ [0-9.]+ \\|${name}\\| ?//p" "${OUT}/build.log" > "${OUT}/${name}.log"
        done
    exit 1
fi
echo "=== tested $(git -C "$REPO_ROOT" rev-parse "$REF") (${REF})" | tee -a "${OUT}/summary.txt"
rc="$(cat "${OUT}/exit-code")"
echo "=== exit ${rc}; logs in ${OUT}"
exit "$rc"
