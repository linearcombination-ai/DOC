/**
 * P6 item E: HTML output for POST /documents, PDF through the Python seam (WeasyPrint, engine swappable), and the other request
 * shapes the core supports (Translation Questions, Translation Words).
 */
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { createStack, judeRequest, PROTO, type Stack } from "./support/stack.ts";

let s: Stack | undefined;
afterEach(async () => {
  await s?.close();
  s = undefined;
});
const VENV_PYTHON = process.env["DOC_VENV_PYTHON"] ?? join(PROTO, "../_exp/venv/bin/python");
const haveWeasy = existsSync(VENV_PYTHON);
const doc = (extra: Record<string, unknown> = {}): Record<string, unknown> => judeRequest({ generate_docx: false, ...extra });
const findingsOf = async (st: Stack, key: string): Promise<{ code: string }[]> => (await (await st.app.app.request(`/findings/documents/${key}`)).json()) as { code: string }[];

describe("POST /documents", () => {
  it("without generate_pdf: the html DOC writes as a by-product is produced and placed; no pdf", async () => {
    s = await createStack();
    await s.startWorker();
    const res = await s.post(doc(), "/documents");
    expect(res.status).toBe(200);
    const done = await s.waitFor(String(res.json["task_id"]));
    expect(done.last["state"]).toBe("SUCCESS");
    const key = String(done.last["result"]);
    const html = readFileSync(join(s.dirs.out, `${key}.html`), "utf8");
    // DOC's own markup is the default (what DOC's end-to-end tests parse): its header template and CSS, then `sup.versemarker` inside `span.verse`
    expect(html.startsWith("<!DOCTYPE html PUBLIC")).toBe(true);
    expect(html).toContain('<style media="all" type="text/css">');
    const body = /<body.*?>(.*?)<\/body>/s.exec(html)?.[1] ?? "";
    expect(body).toContain('class="versemarker"');
    expect(body).toContain('<span class="chaptermarker">1</span>');
    expect(body).toMatch(/Jude/);
    expect(existsSync(join(s.dirs.out, `${key}.pdf`))).toBe(false);
    expect(existsSync(join(s.dirs.out, `${key}.docx`))).toBe(false);
    expect(s.states.get(String(res.json["task_id"]))).not.toContain("Converting to PDF");
  }, 60_000);

  it("OUTPUT_LOOK=proposed: the html is the proposed look (semantic classes, its own CSS), still a complete document", async () => {
    s = await createStack({ env: { OUTPUT_LOOK: "proposed" } });
    await s.startWorker();
    const done = await s.waitFor(String((await s.post(doc(), "/documents")).json["task_id"]));
    expect(done.last["state"]).toBe("SUCCESS");
    const html = readFileSync(join(s.dirs.out, `${String(done.last["result"])}.html`), "utf8");
    expect(html.startsWith("<!doctype html>")).toBe(true);
    expect(html).toContain("<style>");
    expect(html).toContain('class="tn-note"');
  }, 60_000);

  it("PDF_ENGINE=off: generate_pdf answers the typed 501 and nothing is queued", async () => {
    s = await createStack({ env: { PDF_ENGINE: "off" } });
    const res = await s.post(doc({ generate_pdf: true }), "/documents");
    expect(res.status).toBe(501);
    expect(res.json).toMatchObject({ code: "not_implemented", retryable: false });
    expect((await s.app.jobs?.service.health())?.waiting).toBe(0);
  }, 30_000);

  it.skipIf(!haveWeasy)("generate_pdf: a PDF is produced through the Python seam with WeasyPrint, beside the html, with the state `Converting to PDF`", async () => {
    s = await createStack({ env: { PYTHON_BIN: VENV_PYTHON, STAGE_DOCX_DEADLINE_MS: "120000" } });
    await s.startWorker();
    const res = await s.post(doc({ generate_pdf: true }), "/documents");
    const id = String(res.json["task_id"]);
    const done = await s.waitFor(id, ["SUCCESS", "FAILURE"], 120_000);
    expect(done.last["state"]).toBe("SUCCESS");
    const key = String(done.last["result"]);
    const pdf = readFileSync(join(s.dirs.out, `${key}.pdf`));
    expect(pdf.subarray(0, 5).toString("latin1")).toBe("%PDF-");
    expect(pdf.length).toBeGreaterThan(5_000);
    expect(existsSync(join(s.dirs.out, `${key}.html`))).toBe(true);
    expect(s.states.get(id)).toContain("Converting to PDF");
  }, 180_000);

  it("generate_pdf without WeasyPrint in the worker's Python: a permanent typed failure (pdf_unavailable), not a retry loop", async () => {
    // WeasyPrint is hidden from the worker's Python by a stub module that fails to import (works whatever the host's Python has installed); Prince is not installed here either
    const stub = mkdtempSync(join(tmpdir(), "no-weasy-"));
    writeFileSync(join(stub, "weasyprint.py"), 'raise ImportError("WeasyPrint is hidden for this test")\n');
    const before = process.env["PYTHONPATH"];
    process.env["PYTHONPATH"] = stub;
    try {
      s = await createStack({ env: { PYTHON_BIN: process.env["PYTHON_BIN"] ?? "python3", PDF_ENGINE: "weasyprint" } });
      await s.startWorker();
      const id = String((await s.post(doc({ generate_pdf: true }), "/documents")).json["task_id"]);
      const done = await s.waitFor(id, ["SUCCESS", "FAILURE"], 60_000);
      expect(done.last["state"]).toBe("FAILURE");
      expect(done.last["result"]).toMatchObject({ code: "pdf_unavailable", retryable: false });
      expect((await s.app.jobs?.service.queue.getJob(id))?.attemptsMade).toBe(1);
    } finally {
      if (before === undefined) delete process.env["PYTHONPATH"];
      else process.env["PYTHONPATH"] = before;
      rmSync(stub, { recursive: true, force: true });
    }
  }, 60_000);
});

describe("other request shapes", () => {
  it("Translation Questions and Translation Words with the Bible and the notes: all four resources are read, interleaved and rendered (html and docx)", async () => {
    s = await createStack();
    await s.startWorker();
    const request = judeRequest({
      generate_docx: true,
      resource_requests: [
        { lang_code: "en", resource_type: "ulb", book_code: "jud" },
        { lang_code: "en", resource_type: "tn", book_code: "jud" },
        { lang_code: "en", resource_type: "tq", book_code: "jud" },
        { lang_code: "en", resource_type: "tw", book_code: "jud" },
      ],
    });
    const docx = await s.waitFor(String((await s.post(request)).json["task_id"]), ["SUCCESS", "FAILURE"], 90_000);
    expect(docx.last["state"], JSON.stringify(docx.last)).toBe("SUCCESS");
    const key = String(docx.last["result"]);
    expect(key).toMatch(/^en_ulb-tn-tq-tw_jud_/);
    const html = await s.waitFor(String((await s.post({ ...request, generate_docx: false }, "/documents")).json["task_id"]), ["SUCCESS", "FAILURE"], 90_000);
    expect(html.last["state"]).toBe("SUCCESS");
    const htmlKey = String(html.last["result"]);
    const text = readFileSync(join(s.dirs.out, `${htmlKey}.html`), "utf8");
    expect(text).toContain("Who wrote this letter?"); // Translation Questions
    expect(text).toContain("servant of Jesus Christ"); // the Bible text
    expect(text).toMatch(/General Information/); // Translation Notes
    const tw = text;
    expect(tw.length).toBeGreaterThan(5_000);
    const findings = await findingsOf(s, key);
    process.stderr.write(`TIMING shapes: html ${text.length} bytes, findings ${JSON.stringify(findings.map((f) => f.code))}\n`);
  }, 120_000);

  it("a book that is in the Bible repository but not in the notes repository still gets a document, and the missing resource is a finding, not a failure", async () => {
    s = await createStack();
    await s.startWorker();
    const request = judeRequest({
      resource_requests: [
        { lang_code: "en", resource_type: "ulb", book_code: "jud" },
        { lang_code: "xx", resource_type: "tn", book_code: "jud" },
      ],
    });
    const done = await s.waitFor(String((await s.post(request)).json["task_id"]), ["SUCCESS", "FAILURE"], 60_000);
    expect(done.last["state"]).toBe("SUCCESS");
    expect((await findingsOf(s, String(done.last["result"]))).map((f) => f.code)).toContain("ResourceNotInCatalog");
  }, 60_000);
});
