import { z } from "zod";
import { ConfigError, parseEnv } from "./util/configError.ts";

const Bool = z.enum(["true", "false", "1", "0"]).transform((v) => v === "true" || v === "1");
const IntFrom = (def: number) => z.coerce.number().int().positive().default(def);

/** Everything the shell reads from the environment, validated once at start. */
const EnvSchema = z.object({
  HOST: z.string().default("127.0.0.1"),
  PORT: IntFrom(18301),
  LOG_LEVEL: z.enum(["trace", "debug", "info", "warn", "error", "fatal", "silent"]).default("info"),
  /** `as_built` reproduces DOC's FastAPI; `fixed` turns on the fixes for the defects found. */
  DOC_MODE: z.enum(["as_built", "fixed"]).default("as_built"),
  /** Let a request choose its mode with the X-Doc-Mode header (an experiment aid, off by default). */
  ALLOW_MODE_HEADER: Bool.default(false),
  /** JSON object of core flag overrides, e.g. {"kitchen_names":false}. */
  DOC_FLAGS: z.string().default("{}"),

  DATA_API_URL: z.url(),
  USER_AGENT_STR: z.string().default("wa-doc"),
  X_REQUESTED_WITH_VALUE: z.string().default("WA-Tool-Doc"),
  /** DOC's DATA_API_CACHE_TTL_SECONDS: how long a successful answer is reused. */
  DATA_API_CACHE_TTL_SECONDS: IntFrom(180),
  DATA_API_TIMEOUT_MS: IntFrom(15_000),
  DATA_API_MAX_ATTEMPTS: IntFrom(5),
  DATA_API_BACKOFF_BASE_MS: IntFrom(500),
  DATA_API_BACKOFF_CAP_MS: IntFrom(10_000),
  /** Ceiling for one lookup including all retries and waits. */
  DATA_API_DEADLINE_MS: IntFrom(60_000),

  RESOURCE_ASSETS_DIR: z.string().default("assets_download"),
  /** DOC's ASSET_CACHING_PERIOD (minutes): a cached repository younger than this is reused. */
  ASSET_CACHING_PERIOD: IntFrom(30),
  GIT_BIN: z.string().default("git"),
  GIT_CLONE_TIMEOUT_MS: IntFrom(180_000),
  GIT_MAX_ATTEMPTS: IntFrom(4),
  GIT_BACKOFF_BASE_MS: IntFrom(1_000),
  GIT_BACKOFF_CAP_MS: IntFrom(15_000),
  GIT_CLONE_CONCURRENCY: IntFrom(6),
  /** Per-request ceiling for acquiring everything a request needs (generous: a cold English clone is slow). */
  REQUEST_DEADLINE_MS: IntFrom(600_000),

  /** The wasm core: the full module (worker), and the light module the API process loads (no interleaver, markdown or USFM). */
  WASM_PKG_DIR: z.string().default(""),
  WASM_API_PKG_DIR: z.string().default(""),

  BACKEND_CORS_ORIGINS: z.string().default('["http://localhost:5173"]'),
  EN_RG_DIR: z.string().default("backend/passages/data/en_rg"),
});

export type Config = z.infer<typeof EnvSchema> & { corsOrigins: string[]; flagOverrides: Record<string, boolean> };

function jsonSetting<S extends z.ZodType>(name: string, text: string, schema: S, problems: string[]): z.infer<S> | undefined {
  try {
    const parsed = schema.safeParse(JSON.parse(text));
    if (parsed.success) return parsed.data;
    problems.push(`${name}: ${parsed.error.issues.map((i) => i.message).join("; ")} (got ${JSON.stringify(text)})`);
  } catch {
    problems.push(`${name}: not valid JSON (got ${JSON.stringify(text)})`);
  }
  return undefined;
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): Config {
  const parsed = parseEnv(EnvSchema, env);
  const problems: string[] = [];
  const origins = jsonSetting("BACKEND_CORS_ORIGINS", parsed.BACKEND_CORS_ORIGINS, z.array(z.string()), problems);
  const overrides = jsonSetting("DOC_FLAGS", parsed.DOC_FLAGS, z.record(z.string(), z.boolean()), problems);
  if (parsed.DATA_API_BACKOFF_BASE_MS > parsed.DATA_API_BACKOFF_CAP_MS) problems.push(`DATA_API_BACKOFF_BASE_MS=${parsed.DATA_API_BACKOFF_BASE_MS} is larger than DATA_API_BACKOFF_CAP_MS=${parsed.DATA_API_BACKOFF_CAP_MS}`);
  if (parsed.GIT_BACKOFF_BASE_MS > parsed.GIT_BACKOFF_CAP_MS) problems.push(`GIT_BACKOFF_BASE_MS=${parsed.GIT_BACKOFF_BASE_MS} is larger than GIT_BACKOFF_CAP_MS=${parsed.GIT_BACKOFF_CAP_MS}`);
  if (parsed.DATA_API_DEADLINE_MS < parsed.DATA_API_TIMEOUT_MS) problems.push(`DATA_API_DEADLINE_MS=${parsed.DATA_API_DEADLINE_MS} is shorter than one attempt (DATA_API_TIMEOUT_MS=${parsed.DATA_API_TIMEOUT_MS})`);
  if (problems.length > 0 || origins === undefined || overrides === undefined) throw new ConfigError(problems);
  return { ...parsed, corsOrigins: origins, flagOverrides: overrides };
}
