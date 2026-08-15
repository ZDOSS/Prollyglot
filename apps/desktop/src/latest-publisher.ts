/**
 * Publishes one value at a time while retaining only the newest replacement.
 * This prevents a slow IPC or overlay consumer from replaying stale UI frames.
 */
export class LatestPublisher<Value> {
  private pending?: Value;
  private flushing?: Promise<void>;
  private readonly send: (value: Value) => Promise<void>;
  private readonly onError: (error: unknown) => void;

  constructor(
    send: (value: Value) => Promise<void>,
    onError: (error: unknown) => void = () => undefined
  ) {
    this.send = send;
    this.onError = onError;
  }

  publish(value: Value): void {
    this.pending = value;
    this.flushing ??= this.flush();
  }

  async idle(): Promise<void> {
    while (this.flushing) await this.flushing;
  }

  private async flush(): Promise<void> {
    try {
      while (this.pending !== undefined) {
        const current = this.pending;
        this.pending = undefined;
        try {
          await this.send(current);
        } catch (error) {
          this.onError(error);
        }
      }
    } finally {
      this.flushing = undefined;
      if (this.pending !== undefined) this.flushing = this.flush();
    }
  }
}
