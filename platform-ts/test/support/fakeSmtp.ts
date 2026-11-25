/** A real SMTP server on a local port (smtp-server) that keeps every message it accepts, with failure modes for the email tests. */
import type { Socket } from "node:net";
import { SMTPServer } from "smtp-server";

export type SmtpMode = "ok" | "reject_rcpt" | "reject_data" | "drop_after_data" | "slow_data";

export interface ReceivedMessage {
  from: string;
  to: string[];
  raw: string;
}

export interface FakeSmtp {
  port: number;
  messages: ReceivedMessage[];
  setMode(mode: SmtpMode): void;
  close(): Promise<void>;
}

export async function startFakeSmtp(initial: SmtpMode = "ok"): Promise<FakeSmtp> {
  let mode: SmtpMode = initial;
  const messages: ReceivedMessage[] = [];
  const sockets = new Set<Socket>();
  const server = new SMTPServer({
    authOptional: true,
    disabledCommands: ["STARTTLS", "AUTH"],
    logger: false,
    size: 30 * 1024 * 1024,
    onRcptTo(_address, _session, cb) {
      if (mode === "reject_rcpt") return cb(Object.assign(new Error("550 5.1.1 no such user"), { responseCode: 550 }));
      cb();
    },
    onData(stream, session, cb) {
      const chunks: Buffer[] = [];
      stream.on("data", (c: Buffer) => chunks.push(c));
      stream.on("end", () => {
        const raw = Buffer.concat(chunks).toString("utf8");
        if (mode === "reject_data") return cb(Object.assign(new Error("554 5.7.1 message refused"), { responseCode: 554 }));
        messages.push({ from: session.envelope.mailFrom ? session.envelope.mailFrom.address : "", to: session.envelope.rcptTo.map((r) => r.address), raw });
        if (mode === "drop_after_data") {
          // the message is kept (the server has it) but the client never hears so
          for (const s of sockets) s.destroy();
          return;
        }
        if (mode === "slow_data") return void setTimeout(() => cb(), 2_000);
        cb();
      });
    },
  });
  server.server.on("connection", (s: Socket) => {
    sockets.add(s);
    s.on("close", () => sockets.delete(s));
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.server.address();
  const port = typeof address === "object" && address ? address.port : 0;
  return {
    port,
    messages,
    setMode: (m) => {
      mode = m;
    },
    close: () =>
      new Promise<void>((resolve) => {
        for (const s of sockets) s.destroy();
        server.close(() => resolve());
      }),
  };
}
