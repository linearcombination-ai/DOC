import { pino, type Logger } from "pino";

export type Log = Logger;

/** One root logger; components take a child with a `component` field. `debug` stays useful: every
 *  data-API attempt, every git invocation and every seam call is logged there with its duration. */
export function createLogger(level: string, destination?: NodeJS.WritableStream): Log {
  const options = { level, base: { service: "doc-shell-ts" }, timestamp: pino.stdTimeFunctions.isoTime };
  return destination ? pino(options, destination) : pino(options);
}

export function silentLogger(): Log {
  return pino({ level: "silent" });
}
