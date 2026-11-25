#!/usr/bin/env bash
# Lines of code by layer (wc -l; blank lines and comments included; generated files listed separately). Run from anywhere.
cd "$(dirname "${BASH_SOURCE[0]}")/.."
n() { cat "$@" 2>/dev/null | wc -l; }
row() { printf "%-62s %6s\n" "$1" "$2"; }
row "layer" "lines"
row "Rust core: doc-books + doc-model + doc-request + doc-md + doc-interleave + doc-canon (P3b/P4, copied)" "$(n $(find core/crates/doc-books core/crates/doc-model core/crates/doc-request core/crates/doc-md core/crates/doc-interleave core/crates/doc-canon -name '*.rs' | grep -v tables_gen))"
row "Rust core: tables_gen.rs (GENERATED)"                       "$(n core/crates/doc-books/src/tables_gen.rs)"
row "Rust: doc-job + doc-kitchen + doc-wasm (P5/P6 entry points)"  "$(n core/crates/doc-job/src/*.rs core/crates/doc-kitchen/src/*.rs core/crates/doc-wasm/src/*.rs)"
row "TS: contract (zod schemas and routes)"                      "$(n src/contract/*.ts)"
row "TS: HTTP app, API main, worker main, shell config"          "$(n src/app.ts src/main.ts src/worker-main.ts src/config.ts)"
row "TS: core wrapper, catalog client, acquirer, lookup, util"    "$(n src/core/*.ts src/catalog/*.ts src/acquirer/*.ts src/service/*.ts src/util/*.ts)"
row "TS: job runner (queue, worker, pipeline, scratch, errors, config, types, redis)" "$(n src/jobs/queue.ts src/jobs/worker.ts src/jobs/pipeline.ts src/jobs/scratch.ts src/jobs/errors.ts src/jobs/config.ts src/jobs/types.ts src/jobs/redis.ts src/jobs/jobcore.ts)"
row "TS: docx seam (process runner, persistent pool)"             "$(n src/jobs/docx.ts src/jobs/docxpool.ts)"
row "TS: core thread (wasm on a worker thread)"                  "$(n src/jobs/corethread.ts src/jobs/corethread.entry.ts)"
row "TS: cross-worker clone lock"                                "$(n src/jobs/repolock.ts)"
row "TS: email step"                                             "$(n src/jobs/email.ts)"
row "TS: observability (metrics, telemetry)"                     "$(n src/obs/*.ts)"
row "Python: seam (docx_core, docx_worker, docx_server)"         "$(n python/docx_core.py python/docx_worker.py python/docx_server.py)"
row "Python: template-free docx generator (P3b, stdlib only)"    "$(n python/docxgen/*.py)"
row "TS tests (vitest) + support"                                "$(n test/*.ts test/support/*.ts)"
row "tools and scripts (TS, shell, Python)"                      "$(n tools/*.ts tools/*.sh tools/*.py tools/e2e/* tools/doc-celery/*.sh tools/doc-celery/shim/*.py scripts/*.ts scripts/*.sh)"
row "generated: openapi.json / api.d.ts"                         "$(n generated/openapi.json) / $(n generated/api.d.ts)"
echo
echo "tests: $(grep -ho '\bit\(\.skipIf([^)]*)\)\?(' test/*.ts | wc -l) cases in $(ls test/*.test.ts | wc -l) files; Rust #[test]: $(grep -rho '#\[test\]' core/crates | wc -l)"
echo "'any' in TypeScript source and tests (word-boundary grep, prose included): $(grep -rnw 'any' src test --include=*.ts | grep -v -E '^\S+:\s*[0-9]+:\s*(//|\*|/\*)' | grep -E ': any\b|<any>|as any|any\[\]|Record<[^>]*any' | wc -l)"
echo "'typing.Any' in Python: $(grep -rn 'typing.Any\|import Any\|: Any' python tools --include=*.py | grep -v 'no typing.Any' | wc -l)"
