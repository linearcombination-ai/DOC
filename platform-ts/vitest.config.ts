import { defineConfig } from "vitest/config";

// The job tests spawn real processes (redis-server, node workers, python, git) and measure deadlines; two test files in
// parallel interfere (one test's hung `sleep 600` is another test's stray-process check), so files run one at a time.
export default defineConfig({ test: { include: ["test/**/*.test.ts"], exclude: ["vendor/**", "node_modules/**", "core/**"], fileParallelism: false, testTimeout: 60_000, hookTimeout: 60_000 } });
