#!/usr/bin/env bash
# Smoke test of a running stack (compose or plain processes): health, lookups, one docx, one html, one pdf, the UI.
#   API=http://localhost:5005 FILES=http://localhost:8089 UI=http://localhost:8001 OUT=<dir for evidence> bash ci/smoke.sh
# Exit 0 only when everything passed. Prints one line per check with its time. Needs: curl, python3.
set -uo pipefail
API="${API:-http://localhost:5005}"; FILES="${FILES:-http://localhost:8089}"; UI="${UI:-http://localhost:8001}"
OUT="${OUT:-smoke-out}"; mkdir -p "$OUT"
TIMEOUT="${SMOKE_DOC_TIMEOUT:-300}"
fail=0
now() { date +%s.%N; }
ok() { printf 'PASS %-44s %6.2fs %s\n' "$1" "$(echo "$(now) - $2" | bc -l 2>/dev/null || echo 0)" "${3:-}"; }
bad() { printf 'FAIL %-44s %s\n' "$1" "$2"; fail=1; }

check_json() { # name url expr(python on d)
  local t; t=$(now)
  if curl -fsS --max-time 60 "$2" -o "$OUT/$1.json" && python3 -c "import json,sys; d=json.load(open('$OUT/$1.json')); assert $3" 2>"$OUT/$1.err"; then ok "$1" "$t"; else bad "$1" "$(head -c 200 "$OUT/$1.err" 2>/dev/null) $(head -c 200 "$OUT/$1.json" 2>/dev/null)"; fi
}

check_json health-api "$API/health/status" "d[0]['status'] in ('ok','degraded')"
if [ "${SKIP_GATEWAY:-0}" != 1 ]; then t=$(now); if curl -fsS --max-time 10 "$API/gateway/health" >/dev/null; then ok gateway-health "$t"; else bad gateway-health "no answer from $API/gateway/health (not behind the gateway?)"; fi; fi
check_json language-codes "$API/language_codes_and_names" "len(d) > 100"
check_json resource-types "$API/resource_types/en/jud" "len(d) >= 1"

submit() { # name path json -> prints key on success
  local name="$1" path="$2" body="$3" t id key state last="" states=""
  t=$(now)
  id=$(curl -fsS --max-time 30 -X POST "$API$path" -H 'content-type: application/json' -d "$body" | python3 -c "import sys,json; print(json.load(sys.stdin)['task_id'])") || { bad "$name" "POST $path failed"; return 1; }
  local deadline=$(( $(date +%s) + TIMEOUT ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    local resp; resp=$(curl -fsS --max-time 20 "$API/task_status/$id") || { sleep 2; continue; }
    state=$(python3 -c "import sys,json; print(json.loads(sys.argv[1])['state'])" "$resp")
    [ "$state" != "$last" ] && { states="$states|$state"; last="$state"; }
    if [ "$state" = "SUCCESS" ]; then key=$(python3 -c "import sys,json; print(json.loads(sys.argv[1])['result'])" "$resp"); echo "$key" > "$OUT/$name.key"; ok "$name" "$t" "states$states key=$key"; return 0; fi
    if [ "$state" = "FAILURE" ]; then bad "$name" "FAILURE after$states: $resp"; return 1; fi
    sleep 1
  done
  bad "$name" "no SUCCESS within ${TIMEOUT}s (states$states)"; return 1
}

REQ_BASE='"assembly_strategy_kind":"lbo","layout_for_print":false,"generate_epub":false,"limit_words":true,"document_request_source":"ui","resource_requests":[{"lang_code":"en","resource_type":"ulb","book_code":"jud"},{"lang_code":"en","resource_type":"tn","book_code":"jud"}]'
fetch() { # name key suffix minbytes magic
  local t; t=$(now)
  local code; code=$(curl -s --max-time 60 -o "$OUT/$1.$3" -w '%{http_code}' "$FILES/$2.$3")
  local size; size=$(stat -c %s "$OUT/$1.$3" 2>/dev/null || echo 0)
  if [ "$code" = 200 ] && [ "$size" -ge "$4" ] && head -c 8 "$OUT/$1.$3" | grep -q -a "$5"; then ok "download-$1.$3" "$t" "bytes=$size"; else bad "download-$1.$3" "http=$code bytes=$size"; fi
}

if submit docx /documents_docx "{\"email_address\":null,$REQ_BASE,\"generate_pdf\":false,\"generate_docx\":true}"; then fetch docx "$(cat "$OUT/docx.key")" docx 10000 'PK'; fi
if submit html /documents "{\"email_address\":null,$REQ_BASE,\"generate_pdf\":false,\"generate_docx\":false}"; then
  fetch html "$(cat "$OUT/html.key")" html 1000 '<'
  grep -q -a '<body' "$OUT/html.html" 2>/dev/null && ok html-has-body "$(now)" || bad html-has-body "no <body> in the html"
fi
if submit pdf /documents "{\"email_address\":null,$REQ_BASE,\"generate_pdf\":true,\"generate_docx\":false,\"use_prince\":${USE_PRINCE:-false}}"; then fetch pdf "$(cat "$OUT/pdf.key")" pdf 5000 '%PDF'; fi

# A PDF through Prince (set SMOKE_PRINCE=1 where Prince is installed: the worker image): the request asks for it with use_prince, and the document must not
# carry a pdf_engine_fallback finding (that finding means Prince was missing or failed and WeasyPrint made the PDF instead).
if [ "${SMOKE_PRINCE:-0}" = 1 ]; then
  if submit pdf-prince /documents "{\"email_address\":null,$REQ_BASE,\"generate_pdf\":true,\"generate_docx\":false,\"use_prince\":true}"; then
    fetch pdf-prince "$(cat "$OUT/pdf-prince.key")" pdf 5000 '%PDF'
    t=$(now); curl -fsS --max-time 30 "$FILES/$(cat "$OUT/pdf-prince.key").findings.json" -o "$OUT/pdf-prince.findings.json" 2>/dev/null || echo '[]' > "$OUT/pdf-prince.findings.json"
    if grep -q pdf_engine_fallback "$OUT/pdf-prince.findings.json"; then bad pdf-prince-engine "Prince was not used: $(head -c 300 "$OUT/pdf-prince.findings.json")"; else ok pdf-prince-engine "$t" "no fallback finding"; fi
  fi
fi

if [ "${SKIP_UI:-0}" != 1 ]; then
  t=$(now)
  if curl -fsS --max-time 30 "$UI/" -o "$OUT/ui.html" && grep -q -i '<html' "$OUT/ui.html"; then ok ui-home "$t"; else bad ui-home "no HTML from $UI/"; fi
fi

[ "$fail" = 0 ] && echo "SMOKE: all checks passed" || echo "SMOKE: FAILED"
exit "$fail"
