#!/usr/bin/env bash
# Run HTTP router ↔ OpenAPI ↔ catalog drift tests and write a JSON report for CI.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
mkdir -p artifacts/openapi
log="artifacts/openapi/router-drift.log"
report="artifacts/openapi/router-drift.json"
status="ok"
exit_code=0

set +e
cargo test -p templiqx-http --test openapi_drift -- --nocapture >"$log" 2>&1
exit_code=$?
set -e

if [ "$exit_code" -ne 0 ]; then
  status="fail"
fi

python3 - "$report" "$log" "$status" "$exit_code" <<'PY'
import json, pathlib, sys
from datetime import datetime, timezone
report_path, log_path, status, exit_code = sys.argv[1:5]
log = pathlib.Path(log_path).read_text(encoding="utf-8", errors="replace")
tail = "\n".join(log.splitlines()[-80:])
body = {
    "name": "router-drift",
    "generatedAt": datetime.now(timezone.utc).isoformat(),
    "status": status,
    "exitCode": int(exit_code),
    "testPackage": "templiqx-http",
    "testBinary": "openapi_drift",
    "logPath": log_path,
    "logTail": tail,
}
pathlib.Path(report_path).write_text(json.dumps(body, indent=2) + "\n", encoding="utf-8")
PY

echo "router drift report: $report (status=$status)"
exit "$exit_code"
