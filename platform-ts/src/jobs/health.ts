/** A tiny HTTP listener for the worker's container health check (the worker has no public API). Not a metrics endpoint: metrics are fleet-wide in Redis. */
import { createServer, type Server } from "node:http";

export interface WorkerHealth {
  /** false once shutdown has started: the container is then reported unhealthy so nothing sends it new work. */
  accepting: boolean;
  id: string;
  running: () => number;
  done: () => number;
  startedAt: number;
}

export function startHealthServer(port: number, host: string, state: WorkerHealth): Server {
  const server = createServer((req, res) => {
    if (req.method === "GET" && (req.url === "/health" || req.url === "/health/status")) {
      const body = JSON.stringify({ status: state.accepting ? "ok" : "draining", worker: state.id, pid: process.pid, running: state.running(), done: state.done(), uptimeSeconds: Math.round((Date.now() - state.startedAt) / 1000), rssMb: Math.round(process.memoryUsage.rss() / 1048576) });
      res.writeHead(state.accepting ? 200 : 503, { "content-type": "application/json" });
      res.end(body);
      return;
    }
    res.writeHead(404, { "content-type": "text/plain" });
    res.end("not found\n");
  });
  server.listen(port, host);
  return server;
}
