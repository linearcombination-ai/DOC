"""Time DOC's own docx step (`convert_html_to_docx`, python-docx + docxtpl + docxcompose, unpatched at 50de21cd)
on DOC's own interleaver parts for the slice request (the oracle record). Run from the DOC root with the DOC venv.
    python doc_docx_step.py ORACLE.json OUTDIR RUNS
Prints one JSON line: import time, first-call time, warm times, ru_maxrss."""
import json, resource, sys, time
t_import = time.perf_counter()
from doc.domain import document_generator as dg  # noqa: E402
from doc.domain.model import DocumentPart  # noqa: E402
t_import = time.perf_counter() - t_import
oracle = json.load(open(sys.argv[1]))
parts = [DocumentPart(**{k: p[k] for k in ("content", "is_rtl", "add_page_break", "contained_in_two_column_section", "use_section_visual_separator")}) for p in oracle["parts"]]
out = sys.argv[2]
times = []
for i in range(int(sys.argv[3])):
    t = time.perf_counter()
    dg.convert_html_to_docx(f"{out}/x.html", f"{out}/doc{i}.docx", parts, False, "English", "Unlocked Literal Bible, Translation Notes")
    times.append(time.perf_counter() - t)
print(json.dumps({"import_s": t_import, "first_call_s": times[0], "warm_s": times[1:], "maxrss_kb": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss}))
