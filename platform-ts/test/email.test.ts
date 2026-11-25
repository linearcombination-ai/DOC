/**
 * P6 item E, the email step: its own idempotency key, a real SMTP server (smtp-server) in the tests, and the rule that an
 * email must never go out twice after a retry or a redelivery.
 */
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Redis } from "ioredis";
import { afterEach, describe, expect, it } from "vitest";
import { emailRecordKey, isDefiniteFailure, sendEmailOnce, type EmailRequest } from "../src/jobs/email.ts";
import { loadJobConfig } from "../src/jobs/config.ts";
import { MemoryMetrics } from "../src/obs/metrics.ts";
import { silentLogger } from "../src/util/log.ts";
import { startFakeSmtp, type FakeSmtp } from "./support/fakeSmtp.ts";
import { createStack, judeRequest, type Stack } from "./support/stack.ts";

let s: Stack | undefined;
let smtp: FakeSmtp | undefined;
afterEach(async () => {
  await s?.close();
  s = undefined;
  await smtp?.close();
  smtp = undefined;
});
const TMP = process.env["TMPDIR"] ?? tmpdir();
const mailEnv = (port: number): Record<string, string> => ({ SEND_EMAIL: "true", SMTP_HOST: "127.0.0.1", SMTP_PORT: String(port), SMTP_STARTTLS: "off", FROM_EMAIL_ADDRESS: "doc@example.org", EMAIL_SEND_SUBJECT: "Your document", SMTP_TIMEOUT_MS: "3000" });
const withEmail = (to: string, extra: Record<string, unknown> = {}): Record<string, unknown> => judeRequest({ email_address: to, ...extra });
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

describe("the email step through the queue", () => {
  it("a request with an address sends exactly one message with the document attached, and the state `Sending email` is published", async () => {
    smtp = await startFakeSmtp();
    s = await createStack({ env: mailEnv(smtp.port) });
    await s.startWorker();
    const id = String((await s.post(withEmail("reader@example.org"))).json["task_id"]);
    const done = await s.waitFor(id);
    expect(done.last["state"]).toBe("SUCCESS");
    expect(s.states.get(id)).toContain("Sending email");
    expect(smtp.messages.length).toBe(1);
    const m = smtp.messages[0];
    expect(m?.to).toEqual(["reader@example.org"]);
    expect(m?.raw).toContain("Subject: Your document");
    expect(m?.raw).toContain("From: doc@example.org");
    expect(m?.raw).toMatch(/filename="?en_ulb-tn_jud_[0-9a-f]+\.docx"?/);
    expect(m?.raw).toContain("Dear BIEL User,");
    expect(m?.raw).toMatch(/X-Doc-Idempotency-Key: [0-9a-f]{32}/);
    const jobReq = String(((await s.app.jobs?.service.queue.getJob(id))?.data as { requestId?: string } | undefined)?.requestId);
    expect(jobReq).toMatch(/^[0-9a-f-]{12}$/);
    expect(m?.raw).toMatch(new RegExp(`X-Request-Id: ?${jobReq}`, "i")); // nodemailer writes the header as X-Request-ID
  }, 60_000);

  it("a retry after the email was sent does not send it again (the first attempt dies right after the email step)", async () => {
    smtp = await startFakeSmtp();
    const marker = join(mkdtempSync(join(TMP, "doc-mark-")), "m");
    s = await createStack({ env: { ...mailEnv(smtp.port), JOB_FAULT: `after_email_once:${marker}` } });
    await s.startWorker();
    const id = String((await s.post(withEmail("reader@example.org"))).json["task_id"]);
    const done = await s.waitFor(id);
    expect(done.last["state"]).toBe("SUCCESS");
    expect((await s.app.jobs?.service.queue.getJob(id))?.attemptsMade).toBe(2); // attempt 1 died after the email, attempt 2 finished the job
    await sleep(300);
    expect(smtp.messages.length).toBe(1);
    const skipped = s.logs().filter((l) => l["component"] === "email" && String(l["msg"]).startsWith("not sending"));
    expect(skipped.length).toBe(1);
    expect(skipped[0]).toMatchObject({ state: "sent" });
  }, 60_000);

  it("a redelivery (the same job run again by another worker after a lost lock) does not send it again", async () => {
    smtp = await startFakeSmtp();
    s = await createStack({ env: mailEnv(smtp.port) });
    const w = await s.startWorker();
    const id = String((await s.post(withEmail("reader@example.org"))).json["task_id"]);
    await s.waitFor(id);
    expect(smtp.messages.length).toBe(1);
    // BullMQ redelivers a job whose lock was lost: simulate by running the stored job data through the pipeline again, as another worker would
    const job = await s.app.jobs?.service.queue.getJob(id);
    expect(job).toBeDefined();
    const { runDocumentJob } = await import("../src/jobs/pipeline.ts");
    const deps = { config: s.app.config, jobs: s.jobConfig, log: silentLogger(), lookup: s.app.lookup, acquirer: s.app.acquirer, core: s.app.jobCore, docx: s.app.docx as NonNullable<typeof s.app.docx>, redis: s.app.jobs?.service.connection as Redis };
    const ctx = { jobId: id, attempt: 1, maxAttempts: 4, signal: new AbortController().signal, setState: async () => undefined };
    for (let i = 0; i < 3; i += 1) await runDocumentJob(deps, job?.data as never, ctx);
    expect(smtp.messages.length).toBe(1);
    await w.close(true);
  }, 60_000);

  it("a new submission of the same document is a new email (a fresh request, not a redelivery); two recipients are never merged into one job", async () => {
    smtp = await startFakeSmtp();
    s = await createStack({ env: mailEnv(smtp.port) });
    await s.startWorker();
    const a = await s.post(withEmail("a@example.org"));
    const b = await s.post(withEmail("b@example.org"));
    expect(a.json["task_id"]).not.toBe(b.json["task_id"]); // same document key, different recipient: two jobs
    await s.waitFor(String(a.json["task_id"]));
    await s.waitFor(String(b.json["task_id"]));
    const again = await s.post(withEmail("a@example.org"));
    await s.waitFor(String(again.json["task_id"]));
    expect(again.json["task_id"]).not.toBe(a.json["task_id"]);
    const recipients = smtp.messages.map((m) => m.to[0]).sort();
    expect(recipients).toEqual(["a@example.org", "a@example.org", "b@example.org"]);
    // five simultaneous identical submissions for one recipient: one job, one email
    smtp.messages.length = 0;
    const dup = await Promise.all(Array.from({ length: 5 }, () => s?.post(withEmail("c@example.org", { layout_for_print: true }))));
    expect(new Set(dup.map((d) => d?.json["task_id"])).size).toBe(1);
    await s.waitFor(String(dup[0]?.json["task_id"]));
    expect(smtp.messages.filter((m) => m.to[0] === "c@example.org").length).toBe(1);
  }, 90_000);

  it("the mail server refuses the recipient: the document is still produced, the job succeeds, and a finding says the email was not sent", async () => {
    smtp = await startFakeSmtp("reject_rcpt");
    s = await createStack({ env: mailEnv(smtp.port) });
    await s.startWorker();
    const id = String((await s.post(withEmail("nobody@example.org"))).json["task_id"]);
    const done = await s.waitFor(id);
    expect(done.last["state"]).toBe("SUCCESS");
    const findings = (await (await s.app.app.request(`/findings/documents/${String(done.last["result"])}`)).json()) as { code: string }[];
    expect(findings.map((f) => f.code)).toContain("EmailNotSent");
    expect(smtp.messages.length).toBe(0);
  }, 60_000);

  it("SEND_EMAIL off: no email, and an info finding says why", async () => {
    smtp = await startFakeSmtp();
    s = await createStack({ env: { ...mailEnv(smtp.port), SEND_EMAIL: "false" } });
    await s.startWorker();
    const done = await s.waitFor(String((await s.post(withEmail("reader@example.org"))).json["task_id"]));
    expect(done.last["state"]).toBe("SUCCESS");
    expect(smtp.messages.length).toBe(0);
    const findings = (await (await s.app.app.request(`/findings/documents/${String(done.last["result"])}`)).json()) as { code: string }[];
    expect(findings.map((f) => f.code)).toContain("EmailDisabled");
  }, 60_000);
});

describe("sendEmailOnce: what the record does in each failure", () => {
  async function setup(mode: Parameters<FakeSmtp["setMode"]>[0] = "ok"): Promise<{ redis: Redis; deps: Parameters<typeof sendEmailOnce>[0]; request: EmailRequest; metrics: MemoryMetrics; file: string }> {
    smtp = await startFakeSmtp(mode);
    s = await createStack({ env: mailEnv(smtp.port) });
    const dir = mkdtempSync(join(TMP, "doc-mail-"));
    const file = join(dir, "doc.docx");
    (await import("node:fs")).writeFileSync(file, "PK-not-really-a-docx");
    const metrics = new MemoryMetrics();
    const redis = s.app.jobs?.service.connection as Redis;
    return { redis, metrics, file, deps: { jobs: loadJobConfig({ ...s.env }), redis, log: silentLogger(), metrics }, request: { submissionId: "sub-1", to: "r@example.org", documentKey: "k1", attachments: [{ path: file, mime: "application/octet-stream" }], requestId: "rid" } };
  }

  it("a definite failure (RCPT refused) releases the claim, so the next attempt sends and only one message exists", async () => {
    const { deps, request, metrics } = await setup("reject_rcpt");
    expect((await sendEmailOnce(deps, request)).outcome).toBe("failed");
    smtp?.setMode("ok");
    expect((await sendEmailOnce(deps, request)).outcome).toBe("sent");
    expect((await sendEmailOnce(deps, request)).outcome).toBe("duplicate_skipped");
    expect(smtp?.messages.length).toBe(1);
    expect(metrics.value("doc_email_total", { result: "failed" })).toBe(1);
    expect(metrics.value("doc_email_total", { result: "sent" })).toBe(1);
    expect(metrics.value("doc_email_total", { result: "duplicate_skipped" })).toBe(1);
  }, 30_000);

  it("an ambiguous failure (the connection drops after the message was sent) is recorded as unknown and is never retried: at most once", async () => {
    const { deps, request, metrics, redis } = await setup("drop_after_data");
    const first = await sendEmailOnce(deps, request);
    expect(first.outcome).toBe("unknown");
    expect(smtp?.messages.length).toBe(1); // the server did get it
    smtp?.setMode("ok");
    const second = await sendEmailOnce(deps, request);
    expect(second.outcome).toBe("unknown");
    expect(smtp?.messages.length).toBe(1);
    expect(await redis.get(emailRecordKey(deps.jobs.QUEUE_PREFIX, request))).toMatch(/^unknown:/);
    expect(metrics.value("doc_email_total", { result: "unknown" })).toBeGreaterThanOrEqual(2);
  }, 30_000);

  it("a claim left by a process that died mid-send (record says sending) blocks a resend: the documented price of at-most-once", async () => {
    const { deps, request, redis } = await setup();
    await redis.set(emailRecordKey(deps.jobs.QUEUE_PREFIX, request), "sending:deadbeef", "EX", 60);
    const out = await sendEmailOnce(deps, request);
    expect(out.outcome).toBe("duplicate_skipped");
    expect(out.detail).toBe("sending");
    expect(smtp?.messages.length).toBe(0);
  }, 30_000);

  it("twenty concurrent attempts at the same logical email send one message", async () => {
    const { deps, request } = await setup();
    const outcomes = await Promise.all(Array.from({ length: 20 }, () => sendEmailOnce(deps, request)));
    expect(outcomes.filter((o) => o.outcome === "sent").length).toBe(1);
    expect(smtp?.messages.length).toBe(1);
  }, 30_000);

  it("classifying failures: before the server asked for the body is definite; after it is ambiguous unless the server answered the body with an error", () => {
    expect(isDefiniteFailure({ command: "CONN", code: "ESOCKET" }, false)).toBe(true);
    expect(isDefiniteFailure({ command: "RCPT TO", responseCode: 550 }, false)).toBe(true);
    expect(isDefiniteFailure({ command: "CONN", code: "ECONNECTION" }, true)).toBe(false); // closed after the body was sent
    expect(isDefiniteFailure(new Error("socket hang up"), true)).toBe(false);
    expect(isDefiniteFailure({ command: "DATA", responseCode: 554 }, true)).toBe(true);
  });
});
