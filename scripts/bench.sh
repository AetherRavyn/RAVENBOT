#!/usr/bin/env bash
# Measure RAVENBOT's ROADMAP performance targets.
# Usage: ./scripts/bench.sh [path-to-ravenbot-binary]
set -euo pipefail

BIN="${1:-target/release/ravenbot}"
export RAVENBOT_DB="$(mktemp -d)/bench.db"

if [ ! -x "$BIN" ]; then
    echo "Binary not found at $BIN — build first: cargo build --release -p ravenbot --bin ravenbot"
    exit 1
fi

echo "=== RAVENBOT Performance Bench ==="
echo

# 1. Binary size (target < 40 MB)
SIZE=$(du -m "$BIN" | cut -f1)
STATUS=$([ "$SIZE" -lt 40 ] && echo "✓ PASS" || echo "✗ FAIL")
echo "Binary size          : ${SIZE} MB   (target < 40 MB)  $STATUS"

# 2. CLI cold start (target < 300 ms): spawn + open db + migrations + query
START=$(date +%s%N)
"$BIN" list-bots > /dev/null 2>&1
END=$(date +%s%N)
COLD_MS=$(( (END - START) / 1000000 ))
STATUS=$([ "$COLD_MS" -lt 300 ] && echo "✓ PASS" || echo "✗ FAIL")
echo "CLI cold start       : ${COLD_MS} ms  (target < 300 ms) $STATUS"

# 3. MCP server roundtrip (initialize + tools/list)
if command -v python3 > /dev/null; then
    MCP_MS=$(python3 - "$BIN" << 'PYEOF'
import json, subprocess, sys, time

bin_path = sys.argv[1]
start = time.time()
p = subprocess.Popen(
    [bin_path, "mcp-serve"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE,
    text=True,
)
try:
    p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}) + "\n")
    p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}) + "\n")
    p.stdin.flush()
    line1 = p.stdout.readline()
    line2 = p.stdout.readline()
    elapsed = (time.time() - start) * 1000
    tools = json.loads(line2).get("result", {}).get("tools", [])
    print(f"{int(elapsed)}|{len(tools)}")
finally:
    p.terminate()
PYEOF
)
    ROUND_MS=$(echo "$MCP_MS" | cut -d'|' -f1)
    TOOL_COUNT=$(echo "$MCP_MS" | cut -d'|' -f2)
    echo "MCP init+tools/list  : ${ROUND_MS} ms (${TOOL_COUNT} tools exposed)"
else
    echo "MCP roundtrip        : skipped (python3 not found)"
fi

echo
echo "Note: GUI cold start / idle RAM targets need a desktop session:"
echo "  cold start < 300ms, idle RAM < 150MB — measure manually with:"
echo "  /usr/bin/time -v ./target/release/ravenbot  (or your OS equivalent)"
