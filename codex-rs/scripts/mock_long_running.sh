#!/usr/bin/env bash
set -euo pipefail

LOG_DIR="${LOG_DIR:-$(pwd)/tmp/mock}" 
mkdir -p "$LOG_DIR"

long_sleep() {
  while true; do
    echo "[$(date +%H:%M:%S)] mock worker tick" >>"$LOG_DIR/mock.log"
    sleep 15
  done
}

echo "[mock] booting services"
long_sleep &
WORKER_PID=$!

cleanup() {
  echo "[mock] shutting down"
  kill "$WORKER_PID" 2>/dev/null || true
  wait "$WORKER_PID" 2>/dev/null || true
}
trap cleanup EXIT

echo "[mock] services running (pid=$WORKER_PID). Press Ctrl+C to stop."
wait
