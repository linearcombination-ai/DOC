/**
 * In-process single flight: concurrent callers with the same key share one execution.
 * No file locks (the file-lock approach did not work well in DOC): the lock is a Map entry.
 * The entry is removed when the work settles, so a failure is never remembered.
 */
export class SingleFlight<T> {
  readonly #inflight = new Map<string, Promise<T>>();

  /** Callers that joined an existing flight rather than starting one. */
  joined = 0;

  run(key: string, work: () => Promise<T>): Promise<T> {
    const existing = this.#inflight.get(key);
    if (existing) {
      this.joined += 1;
      return existing;
    }
    const promise = work().finally(() => {
      this.#inflight.delete(key);
    });
    this.#inflight.set(key, promise);
    return promise;
  }

  get size(): number {
    return this.#inflight.size;
  }
}
