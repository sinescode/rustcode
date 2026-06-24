#!/usr/bin/env bash
# ==============================================================================
# Blazecode CI Status Checker
# Called by cron every 5 minutes.
# Sends Telegram notification if tests are failing.
# NO_REPLY pattern: we just log and exit cleanly when all is well.
# ==============================================================================

PROJECT_DIR="/root/opencodesport/blazecode"
LOGDIR="${PROJECT_DIR}/logs"
STATE_FILE="${LOGDIR}/ci-loop-state.json"
STATUS_LOG="${LOGDIR}/ci-status.log"

if [ ! -f "${STATE_FILE}" ]; then
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] No state file yet — CI loop may still be starting." >> "${STATUS_LOG}"
    exit 0
fi

STATUS=$(jq -r '.status // "unknown"' "${STATE_FILE}" 2>/dev/null)
ITERATION=$(jq -r '.iteration // 0' "${STATE_FILE}" 2>/dev/null)
MESSAGE=$(jq -r '.message // ""' "${STATE_FILE}" 2>/dev/null)
TIMESTAMP=$(jq -r '.timestamp // ""' "${STATE_FILE}" 2>/dev/null)

echo "[$(date '+%Y-%m-%d %H:%M:%S')] Status=${STATUS} iter=${ITERATION} msg=${MESSAGE}" >> "${STATUS_LOG}"

if [ "${STATUS}" = "pass" ]; then
    # All good — nothing to report
    exit 0
fi

if [ "${STATUS}" = "fix-needed" ] || [ "${STATUS}" = "fail" ]; then
    # Check if we already notified recently
    LAST_NOTIFY="${LOGDIR}/.last-notify"
    NOW=$(date +%s)
    if [ -f "${LAST_NOTIFY}" ]; then
        LAST_TIME=$(cat "${LAST_NOTIFY}")
        ELAPSED=$(( NOW - LAST_TIME ))
        if [ ${ELAPSED} -lt 300 ]; then
            # Already notified within last 5 minutes — skip
            exit 0
        fi
    fi

    date +%s > "${LAST_NOTIFY}"

    # Read the fix report if available
    FIX_REPORT="${LOGDIR}/fix-report.json"
    EXTRA=""
    if [ -f "${FIX_REPORT}" ]; then
        FTYPE=$(jq -r '.type // "unknown"' "${FIX_REPORT}")
        FCOUNT=$(jq -r '.failure_count // 0' "${FIX_REPORT}")
        FTESTS=$(jq -r '.failed_tests // [] | join(", ")' "${FIX_REPORT}" 2>/dev/null)
        if [ "${FTYPE}" = "test-failure" ] && [ -n "${FTESTS}" ] && [ "${FTESTS}" != "" ]; then
            EXTRA="
Failed tests: ${FTESTS}"
        fi
    fi

    # Construct notification and send via openclaw's session
    NOTIFY_MSG="🦀 **Blazecode CI Alert**
━━━━━━━━━━━━━
Status: **${STATUS}**
Iteration: #${ITERATION}
Time: ${TIMESTAMP}
${MESSAGE}${EXTRA}

The CI loop will keep retrying. Fix incoming."

    echo "${NOTIFY_MSG}" >> "${STATUS_LOG}"

    # Use openclaw to send a system event — this will wake up my next heartbeat
    # The session system event will inform me on next heartbeat poll
    openclaw gateway \
        --no-color \
        session send \
        --session agent:main:main \
        --text "${NOTIFY_MSG}" \
        2>/dev/null || true
fi
