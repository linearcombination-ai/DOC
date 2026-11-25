/** Exponential backoff with full jitter: a delay drawn uniformly from [0, min(cap, base * 2^attempt)]. */
export interface BackoffPolicy {
  baseMs: number;
  capMs: number;
}

export function backoffDelayMs(policy: BackoffPolicy, retryIndex: number, random: () => number): number {
  const ceiling = Math.min(policy.capMs, policy.baseMs * 2 ** retryIndex);
  return Math.floor(random() * ceiling);
}

export type Sleep = (ms: number, signal?: AbortSignal) => Promise<void>;

export const realSleep: Sleep = (ms, signal) =>
  new Promise<void>((resolve, reject) => {
    if (signal?.aborted) {
      reject(signal.reason);
      return;
    }
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        reject(signal.reason);
      },
      { once: true },
    );
  });
