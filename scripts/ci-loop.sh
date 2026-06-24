#!/usr/bin/env bash
# ==============================================================================
# Blazecode CI Loop — 24/7 nonstop build+test+fix cycle
# ==============================================================================
# Runs continuously: build → test → fail → fix → rebuild → retest → repeat
# Systemd restarts it if killed. Never stops.
# ==============================================================================
set -o pipefail

# Ensure cargo is on PATH
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
[ -f "/root/.cargo/env" ] && source "/root/.cargo/env"

PROJECT_DIR="/root/opencodesport/blazecode"
LOGDIR="${PROJECT_DIR}/logs"
DEBUG_LOGFILE="${LOGDIR}/ci-loop-debug.log"
ERROR_LOGFILE="${LOGDIR}/ci-loop-errors.log"
STATE_FILE="${LOGDIR}/ci-loop-state.json"
LOCKFILE="${LOGDIR}/ci-loop.lock"
FIX_REPORT="${LOGDIR}/fix-report.json"
ITERATION=0
MAX_AGE=$(( 8 * 60 * 60 ))

mkdir -p "${LOGDIR}"

log()  { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "${DEBUG_LOGFILE}"; }
loge() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] ERROR: $*" | tee -a "${ERROR_LOGFILE}" "${DEBUG_LOGFILE}"; }

exec 9>"${LOCKFILE}"
flock -n 9 || { echo "Another ci-loop is running. Exiting."; exit 1; }

write_state() {
    local status=$1 iter=$2 tests=$3 fails=$4 msg=$5
    cat > "${STATE_FILE}" <<-EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "status": "${status}",
  "iteration": ${iter},
  "tests": ${tests},
  "failures": ${fails},
  "message": "${msg}"
}
EOF
}

run_with_log() {
    local label=$1 cmd=$2 outfile="${LOGDIR}/last-${label// /-}.log"
    echo "--- [$(date '+%Y-%m-%d %H:%M:%S')] ${label} ---" >> "${outfile}"
    echo "Command: ${cmd}" >> "${outfile}"
    eval "${cmd}" >> "${outfile}" 2>&1
    local rc=$?
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] Exit code: ${rc}" >> "${outfile}"
    return ${rc}
}

auto_fix() {
    log "→ Attempting auto-fix with cargo fix..."
    cd "${PROJECT_DIR}" || return 1
    cargo fix --all-targets --all-features --allow-dirty 2>&1 | tail -30 >> "${DEBUG_LOGFILE}"
    local rc=$?
    [ ${rc} -eq 0 ] && log "  cargo fix succeeded" || log "  cargo fix rc=${rc}"
    return ${rc}
}

parse_test_counts() {
    local logfile=$1
    local line tests=0 failures=0
    [ ! -f "${logfile}" ] && { echo "0 0"; return; }
    line=$(grep -E "^test result:" "${logfile}" | tail -1)
    [ -n "${line}" ] || { echo "0 0"; return; }
    tests=$(echo "${line}" | sed -n 's/.* \([0-9]*\) passed.*/\1/p')
    failures=$(echo "${line}" | sed -n 's/.* \([0-9]*\) failed.*/\1/p')
    echo "${tests:-0} ${failures:-0}"
}

spawn_fix_request() {
    local logfile=$1 errors=$2
    local error_block
    error_block=$(grep -A5 "^error\[" "${logfile}" 2>/dev/null | head -100)
    cat > "${FIX_REPORT}" <<-EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "iteration": ${ITERATION},
  "project": "blazecode",
  "type": "compilation",
  "auto_fix_tried": true,
  "auto_fix_succeeded": false
}
EOF
    log "→ Fix report written to ${FIX_REPORT}"
    write_state "fix-needed" "${ITERATION}" 0 0 "Compilation errors. Fix report ready."
}

graceful_sleep() {
    local seconds=$1 stop_file="${LOGDIR}/.stop"
    for ((i=0; i<seconds; i++)); do
        [ -f "${stop_file}" ] && { log "Stop file detected. Exiting."; rm -f "${stop_file}"; exit 0; }
        sleep 1
    done
}

log "=============================================="
log "  BLAZECODE CI LOOP STARTED"
log "  $(date)"
log "  PID: $$"
log "=============================================="

while true; do
    ITERATION=$((ITERATION + 1))
    START_EPOCH=$(date +%s)
    log ""
    log "━━━━━ Iteration #${ITERATION} ━━━━━"

    cd "${PROJECT_DIR}" || { loge "Cannot cd to project dir"; graceful_sleep 30; continue; }

    # ── STEP 1: cargo build ────────────────────────────────────────
    log "→ cargo build --workspace --all-targets..."
    write_state "building" "${ITERATION}" 0 0 "Building..."
    run_with_log "cargo-build" "cargo build --workspace --all-targets 2>&1"
    BUILD_RC=$?

    if [ ${BUILD_RC} -ne 0 ]; then
        log "  BUILD FAILED! Trying auto-fix..."
        auto_fix
        run_with_log "cargo-build-after-fix" "cargo build --workspace --all-targets 2>&1"
        BUILD_RC=$?
        if [ ${BUILD_RC} -ne 0 ]; then
            loge "Build still failing after auto-fix."
            spawn_fix_request "${LOGDIR}/last-cargo-build.log" "$(grep '^error\[' "${LOGDIR}/last-cargo-build.log" | head -20)"
            graceful_sleep 30
            continue
        fi
        log "  Auto-fix resolved build!"
    fi

    # ── STEP 2: cargo test ─────────────────────────────────────────
    log "→ cargo test --workspace..."
    write_state "testing" "${ITERATION}" 0 0 "Running all tests..."
    run_with_log "cargo-test" "cargo test --workspace 2>&1"
    TEST_RC=$?

    COUNTS=$(parse_test_counts "${LOGDIR}/last-cargo-test.log")
    TEST_COUNT="${COUNTS%% *}"
    FAIL_COUNT="${COUNTS##* }"

    if [ ${TEST_RC} -eq 0 ]; then
        log "  ✅ ALL ${TEST_COUNT} TESTS PASSED"
        write_state "pass" "${ITERATION}" "${TEST_COUNT}" 0 "All ${TEST_COUNT} tests passed."
        date +%s > "${LOGDIR}/.last-success-timestamp"
        rm -f "${FIX_REPORT}" 2>/dev/null
        graceful_sleep 5
    else
        loge "  ❌ TESTS FAILED! (${FAIL_COUNT} failed out of ${TEST_COUNT})"
        write_state "fail" "${ITERATION}" "${TEST_COUNT}" "${FAIL_COUNT}" "${FAIL_COUNT} test(s) failed."

        FAIL_LIST=$(grep "^test.*FAILED" "${LOGDIR}/last-cargo-test.log" 2>/dev/null | head -20)

        auto_fix
        run_with_log "cargo-build-after-fix" "cargo build --workspace --all-targets 2>&1"
        if [ $? -eq 0 ]; then
            run_with_log "cargo-test-after-fix" "cargo test --workspace 2>&1"
            TEST_RC=$?
            if [ ${TEST_RC} -eq 0 ]; then
                log "  ✅ Auto-fix resolved test failures!"
                write_state "pass" "${ITERATION}" "${TEST_COUNT}" 0 "Tests passed after auto-fix."
                graceful_sleep 5
                continue
            fi
        fi

        loge "Tests still failing after auto-fix. Will retry."
        cat > "${FIX_REPORT}" <<-EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "iteration": ${ITERATION},
  "project": "blazecode",
  "type": "test-failure",
  "test_count": ${TEST_COUNT},
  "failure_count": ${FAIL_COUNT},
  "auto_fix_tried": true
}
EOF
        # Short sleep before retrying - loop never stops
        graceful_sleep 30
    fi

    # Restart fresh every 8h
    [ $(( $(date +%s) - START_EPOCH )) -gt ${MAX_AGE} ] && exec "$0" "$@"
done
