/**
 * A stand-in for the BIEL GraphQL data API: serves a recorded snapshot and can misbehave on
 * demand. Used by the oracle runs (DOC's FastAPI and this shell read the same recorded catalog)
 * and by the fault-injection tests.
 */
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";

export type Mode =
  | { kind: "ok" }
  | { kind: "down" } // the socket is destroyed before any response
  | { kind: "slow"; delayMs: number } // answers correctly, late
  | { kind: "http500" }
  | { kind: "malformed" } // 200, body is not JSON
  | { kind: "wrongShape" } // 200, JSON without data.git_repo
  | { kind: "graphqlError" } // 200, {"errors":[...]}
  | { kind: "badRow" } // 200, valid envelope, one row with a null english_name
  | { kind: "flaky"; failFirst: number }; // 500 for the first N requests, then ok

export interface Snapshot {
  primary: string; // the recorded response body for DOC's primary query
  nonPrimary: string; // ... and for the bg query
}

export interface MockDataApi {
  url: string;
  setMode(mode: Mode): void;
  requests(): { count: number; userAgents: string[]; requestedWith: string[] };
  resetCounters(): void;
  close(): Promise<void>;
  server: Server;
}

function body(req: IncomingMessage): Promise<string> {
  return new Promise((resolve) => {
    const chunks: Buffer[] = [];
    req.on("data", (c: Buffer) => chunks.push(c));
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
  });
}

export async function startMockDataApi(snapshot: Snapshot, port = 0): Promise<MockDataApi> {
  let mode: Mode = { kind: "ok" };
  let count = 0;
  let served = 0; // requests seen since the mode was set (for flaky)
  const userAgents: string[] = [];
  const requestedWith: string[] = [];

  const server = createServer(async (req: IncomingMessage, res: ServerResponse) => {
    const url = new URL(req.url ?? "/", "http://x");
    if (url.pathname === "/__mode") {
      mode = JSON.parse(url.searchParams.get("m") ?? '{"kind":"ok"}') as Mode;
      served = 0;
      res.end("ok");
      return;
    }
    const text = await body(req);
    count += 1;
    served += 1;
    userAgents.push(String(req.headers["user-agent"] ?? ""));
    requestedWith.push(String(req.headers["x-requested-with"] ?? ""));
    const isPrimary = text.includes("Primary");
    const ok = isPrimary ? snapshot.primary : snapshot.nonPrimary;
    const send = (status: number, payload: string): void => {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(payload);
    };
    switch (mode.kind) {
      case "ok":
        return send(200, ok);
      case "down":
        req.socket.destroy();
        return;
      case "slow":
        setTimeout(() => send(200, ok), mode.delayMs);
        return;
      case "http500":
        return send(500, '{"error":"boom"}');
      case "malformed":
        return send(200, "<html>not json");
      case "wrongShape":
        return send(200, '{"data":{"something_else":[]}}');
      case "graphqlError":
        return send(200, '{"errors":[{"message":"field not found"}]}');
      case "badRow": {
        const parsed = JSON.parse(ok) as { data: { git_repo: { content: { language: { english_name: string | null } | null } }[] } };
        const row = parsed.data.git_repo.find((r) => r.content.language !== null);
        if (row?.content.language) row.content.language.english_name = null;
        return send(200, JSON.stringify(parsed));
      }
      case "flaky":
        return served <= mode.failFirst ? send(500, '{"error":"warming up"}') : send(200, ok);
    }
  });
  await new Promise<void>((resolve) => server.listen(port, "127.0.0.1", resolve));
  const address = server.address() as AddressInfo;
  return {
    url: `http://127.0.0.1:${address.port}/v1/graphql`,
    server,
    setMode: (m) => {
      mode = m;
      served = 0;
    },
    requests: () => ({ count, userAgents, requestedWith }),
    resetCounters: () => {
      count = 0;
      userAgents.length = 0;
      requestedWith.length = 0;
    },
    close: () =>
      new Promise<void>((resolve) => {
        server.closeAllConnections();
        server.close(() => resolve());
      }),
  };
}
