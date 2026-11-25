/**
 * The email step, with its own idempotency (the queue is at-least-once; an email is not harmless to repeat).
 *
 * One record per (submission, recipient, document) in Redis, written before anything is sent:
 *
 *   absent            nothing tried yet                          -> claim it (`SET NX`), send
 *   sending:<token>   a send is in progress, or its process died  -> do NOT send again (at most once: see below)
 *   sent              the server accepted the message             -> never send again (kept EMAIL_RECORD_DAYS)
 *   unknown           the outcome could not be established        -> never send again, logged and counted
 *
 * A *definite* failure (connection refused, TLS or auth failure, recipient or envelope rejected, `DATA` answered with an error)
 * means the server did not take the message: the claim is released so the next attempt sends. An *ambiguous* failure (the
 * connection dropped or timed out after the message body was sent) leaves it `unknown`, because sending again could deliver twice.
 * The one window this cannot close is a process killed between the server's acceptance and the write of `sent`: the record then
 * still says `sending`, so a redelivery skips the send. The price is that such a message may never be sent (the document itself
 * is unaffected); the choice is at-most-once, deliberately, because "never twice" is the stated rule.
 *
 * The message carries a `Message-ID` derived from the idempotency key, so even a duplicate that did get through is recognisable
 * downstream, and the failure of this step never fails the document: the caller records a finding.
 */
import { createHash, randomBytes } from "node:crypto";
import { stat } from "node:fs/promises";
import { basename } from "node:path";
import type { Redis } from "ioredis";
import nodemailer, { type SendMailOptions } from "nodemailer";
import type { Metrics } from "../obs/metrics.ts";
import type { Log } from "../util/log.ts";
import type { JobConfig } from "./config.ts";

export interface Attachment {
  path: string;
  mime: string;
}

export type EmailOutcome = "sent" | "duplicate_skipped" | "unknown" | "disabled" | "failed";

export interface EmailRequest {
  /** What makes one logical email: the submission id (stable across retries and redeliveries), the recipient, the document key. */
  submissionId: string;
  to: string;
  documentKey: string;
  attachments: readonly Attachment[];
  requestId: string;
}

export interface EmailDeps {
  jobs: JobConfig;
  redis: Redis;
  log: Log;
  metrics: Metrics;
  /** Test hook: replace the transport (default: nodemailer SMTP from the configuration). */
  transport?: { sendMail(message: SendMailOptions): Promise<unknown> };
}

export const emailRecordKey = (prefix: string, request: Pick<EmailRequest, "submissionId" | "to" | "documentKey">): string =>
  `${prefix}:email:${createHash("sha256").update(`${request.submissionId}\n${request.to.toLowerCase()}\n${request.documentKey}`).digest("hex").slice(0, 40)}`;

/** Release a claim only if it is still ours. */
const RELEASE = `if redis.call('get', KEYS[1]) == ARGV[1] then return redis.call('del', KEYS[1]) else return 0 end`;
/** Replace a claim with a final state only if it is still ours. */
const FINISH = `if redis.call('get', KEYS[1]) == ARGV[1] then redis.call('set', KEYS[1], ARGV[2], 'EX', ARGV[3]) return 1 else return 0 end`;

/**
 * Did the failure happen before the server could have accepted the message? The SMTP client tells us the phase through its debug
 * log: once the server has answered `354` to DATA the body is on its way, and a failure after that (a dropped connection, a timeout)
 * is ambiguous. A failure before it (refused connection, TLS or authentication failure, a rejected sender or recipient) is definite,
 * and so is a server that answers the body with an error code (`DATA` with a 4xx or 5xx response). nodemailer's own `command` field
 * is not usable for this: a connection closed after DATA reports command `CONN`.
 */
export function isDefiniteFailure(error: unknown, dataStarted: boolean): boolean {
  if (!dataStarted) return true;
  const e = error as { command?: unknown; responseCode?: unknown };
  return e.command === "DATA" && typeof e.responseCode === "number" && e.responseCode >= 400;
}

type Transport = NonNullable<EmailDeps["transport"]>;

/** A per-send transport that records whether the DATA phase began (see `isDefiniteFailure`). */
function createTransport(jobs: JobConfig): Transport & { dataStarted: () => boolean } {
  let dataStarted = false;
  const record = (entry: Record<string, unknown>, message: unknown): void => {
    if (entry["tnx"] === "server" && typeof message === "string" && message.startsWith("354")) dataStarted = true;
  };
  const noop = (): void => undefined;
  const logger = { level: noop, trace: noop, debug: record, info: noop, warn: noop, error: noop, fatal: noop };
  const transport = nodemailer.createTransport({
    host: jobs.SMTP_HOST,
    port: jobs.SMTP_PORT,
    secure: jobs.SMTP_SECURE,
    ...(jobs.SMTP_STARTTLS === "require" ? { requireTLS: true } : {}),
    ...(jobs.SMTP_STARTTLS === "off" ? { ignoreTLS: true } : {}),
    ...(jobs.SMTP_PASSWORD !== "" ? { auth: { user: jobs.SMTP_USER || jobs.FROM_EMAIL_ADDRESS, pass: jobs.SMTP_PASSWORD } } : {}),
    connectionTimeout: jobs.SMTP_TIMEOUT_MS,
    greetingTimeout: jobs.SMTP_TIMEOUT_MS,
    socketTimeout: jobs.SMTP_TIMEOUT_MS,
    logger,
    debug: true,
  } as never);
  return { sendMail: (message) => transport.sendMail(message), dataStarted: () => dataStarted };
}

/** The same text DOC's `templates/text/email.txt` renders. */
export const emailBody = (documentKey: string): string => `Dear BIEL User,\n\nThe document you have requested:\n\n${documentKey}\n\nis attached to this email.\n`;

export async function sendEmailOnce(deps: EmailDeps, request: EmailRequest): Promise<{ outcome: EmailOutcome; detail?: string }> {
  const { jobs, redis, log, metrics } = deps;
  if (!jobs.SEND_EMAIL || jobs.SMTP_HOST === "") return { outcome: "disabled" };
  const key = emailRecordKey(jobs.QUEUE_PREFIX, request);
  const elog = log.child({ component: "email", reqId: request.requestId, record: key.slice(-8) });
  const token = randomBytes(8).toString("hex");
  const claim = `sending:${token}`;
  const ttl = jobs.EMAIL_RECORD_DAYS * 86_400;
  const claimed = await redis.set(key, claim, "EX", ttl, "NX");
  if (claimed !== "OK") {
    const state = (await redis.get(key)) ?? "gone";
    const outcome = state.startsWith("unknown") ? "unknown" : "duplicate_skipped";
    metrics.inc("doc_email_total", { result: state.startsWith("sent") ? "duplicate_skipped" : state.startsWith("unknown") ? "unknown" : "in_doubt_skipped" });
    elog.warn({ state: state.split(":")[0] }, "not sending: this email was already sent, is being sent, or its outcome is unknown");
    return { outcome, detail: state.split(":")[0] ?? "" };
  }
  const attachments: { filename: string; path: string; contentType: string }[] = [];
  for (const a of request.attachments) {
    const info = await stat(a.path).catch(() => undefined);
    if (!info) continue;
    if (info.size > jobs.EMAIL_ATTACH_MAX_MB * 1024 * 1024) {
      elog.warn({ file: basename(a.path), bytes: info.size }, "attachment is larger than EMAIL_ATTACH_MAX_MB; sending without it");
      continue;
    }
    attachments.push({ filename: basename(a.path), path: a.path, contentType: a.mime });
  }
  const transport: Transport & { dataStarted?: () => boolean } = deps.transport ?? createTransport(jobs);
  const messageId = `<${key.slice(-32)}@doc.invalid>`;
  try {
    await transport.sendMail({
      from: jobs.FROM_EMAIL_ADDRESS,
      to: request.to,
      subject: jobs.EMAIL_SEND_SUBJECT,
      text: emailBody(request.documentKey),
      attachments,
      messageId,
      headers: { "X-Doc-Idempotency-Key": key.slice(-32), "X-Request-Id": request.requestId },
    });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    if (isDefiniteFailure(error, transport.dataStarted?.() ?? false)) {
      await redis.eval(RELEASE, 1, key, claim);
      metrics.inc("doc_email_total", { result: "failed" });
      elog.warn({ err: detail }, "the mail server did not take the message; the claim is released so a retry can send");
      return { outcome: "failed", detail };
    }
    await redis.eval(FINISH, 1, key, claim, `unknown:${Date.now()}`, String(ttl));
    metrics.inc("doc_email_total", { result: "unknown" });
    elog.error({ err: detail }, "the outcome of the send is unknown (the connection failed after the message was sent); it will not be sent again");
    return { outcome: "unknown", detail };
  }
  await redis.eval(FINISH, 1, key, claim, `sent:${Date.now()}`, String(ttl));
  metrics.inc("doc_email_total", { result: "sent" });
  elog.info({ to: request.to.replace(/^(.).*(@.*)$/, "$1***$2"), attachments: attachments.length }, "email sent");
  return { outcome: "sent" };
}
