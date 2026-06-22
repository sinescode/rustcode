#!/usr/bin/env bash
# Retry a command indefinitely when rate-limited, with exponential backoff
# Usage: retry.sh <command...>

set -euo pipefail

CMD=("$@")
MAX_BACKOFF=60  # max 60 seconds between retries

echo "[retry] Starting: ${CMD[*]}"

for attempt in $(seq 1 9999); do
    if "${CMD[@]}" 2>&1; then
        echo "[retry] Succeeded on attempt $attempt"
        exit 0
    fi

    EXIT_CODE=$?
    
    # Check if it's a rate limit error
    if [[ "$EXIT_CODE" -eq 124 ]] || grep -qi "rate.limit\|too many\|429\|try again" <<< "$OUTPUT" 2>/dev/null; then
        BACKOFF=$((attempt < MAX_BACKOFF ? attempt : MAX_BACKOFF))
        SLEEP=$(( (RANDOM % BACKOFF) + 5 ))
        echo "[retry] Rate limited on attempt $attempt, sleeping ${SLEEP}s..."
        sleep "$SLEEP"
    else
        echo "[retry] Non-rate-limit failure (exit=$EXIT_CODE) on attempt $attempt, retrying in 5s..."
        sleep 5
    fi
done
