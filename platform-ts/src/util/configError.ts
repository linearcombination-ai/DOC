import type { z } from "zod";

/** A configuration that cannot work. Carries every problem found, not just the first (an operator fixes them in one pass). */
export class ConfigError extends Error {
  readonly problems: readonly string[];
  constructor(problems: readonly string[]) {
    super(`invalid configuration:\n${problems.map((p) => `  - ${p}`).join("\n")}`);
    this.name = "ConfigError";
    this.problems = problems;
  }
}

/** Parse the environment with a zod object schema; every failing variable is reported with the value it had (secrets masked). */
export function parseEnv<S extends z.ZodObject>(schema: S, env: NodeJS.ProcessEnv): z.infer<S> {
  const result = schema.safeParse(env);
  if (result.success) return result.data;
  const problems = result.error.issues.map((issue) => {
    const name = String(issue.path[0] ?? "(unknown)");
    const raw = env[name];
    const shown = raw === undefined ? "not set" : /PASSWORD|SECRET|TOKEN|KEY/i.test(name) ? "set (value hidden)" : `got ${JSON.stringify(raw)}`;
    return `${name}: ${issue.message} (${shown})`;
  });
  throw new ConfigError(problems);
}

/** Print a configuration failure the way an operator wants it (one JSON line for the log pipeline, then readable lines) and exit 78 (EX_CONFIG). */
export function exitOnConfigError(error: unknown): never {
  if (error instanceof ConfigError) {
    process.stderr.write(`${JSON.stringify({ level: 60, time: new Date().toISOString(), service: "doc-shell-ts", msg: "invalid configuration, not starting", problems: error.problems })}\n`);
    process.stderr.write(`${error.message}\n`);
    process.exit(78);
  }
  throw error;
}
