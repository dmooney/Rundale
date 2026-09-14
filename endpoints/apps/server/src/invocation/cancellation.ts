import type { Notification, Pool, PoolClient } from "pg";

const cancellationChannel = "parish_stream_cancellation_v1";
const pendingCancellationTtlMs = 30_000;

export type StreamCancellation = () => void;

export interface InvocationCancellationCoordinator {
  register(key: string, cancellation: StreamCancellation): () => void;
  cancel(key: string): Promise<void>;
  close(): Promise<void>;
}

export function streamCancellationKey(requestId: string, attemptId: string): string {
  return JSON.stringify([requestId, attemptId]);
}

export class LocalInvocationCancellationCoordinator implements InvocationCancellationCoordinator {
  private readonly active = new Map<string, Set<StreamCancellation>>();
  private readonly pending = new Map<string, number>();

  register(key: string, cancellation: StreamCancellation): () => void {
    this.removeExpiredPending();
    const expiresAt = this.pending.get(key);
    if (expiresAt !== undefined && expiresAt >= Date.now()) {
      this.pending.delete(key);
      queueMicrotask(cancellation);
    }
    const registrations = this.active.get(key) ?? new Set<StreamCancellation>();
    registrations.add(cancellation);
    this.active.set(key, registrations);
    return () => {
      registrations.delete(cancellation);
      if (registrations.size === 0) this.active.delete(key);
    };
  }

  async cancel(key: string): Promise<void> {
    const registrations = this.active.get(key);
    if (registrations === undefined || registrations.size === 0) {
      this.pending.set(key, Date.now() + pendingCancellationTtlMs);
      return;
    }
    for (const cancellation of registrations) cancellation();
  }

  async close(): Promise<void> {
    this.active.clear();
    this.pending.clear();
  }

  private removeExpiredPending(): void {
    const now = Date.now();
    for (const [key, expiresAt] of this.pending) {
      if (expiresAt < now) this.pending.delete(key);
    }
  }
}

/// Broadcasts authenticated Stop requests to every live Cloud Run instance.
/// PostgreSQL notifications are ephemeral by design; the local coordinator
/// retains a short pending window so a cancellation racing stream registration
/// still wins on every already-running instance.
export class PostgresInvocationCancellationCoordinator implements InvocationCancellationCoordinator {
  private readonly local = new LocalInvocationCancellationCoordinator();
  private listener: PoolClient | undefined;

  private readonly onNotification = (notification: Notification) => {
    if (notification.channel !== cancellationChannel || notification.payload === undefined) return;
    void this.local.cancel(notification.payload);
  };

  private constructor(private readonly pool: Pool) {}

  static async create(pool: Pool): Promise<PostgresInvocationCancellationCoordinator> {
    const coordinator = new PostgresInvocationCancellationCoordinator(pool);
    const listener = await pool.connect();
    listener.on("notification", coordinator.onNotification);
    await listener.query(`LISTEN ${cancellationChannel}`);
    coordinator.listener = listener;
    return coordinator;
  }

  register(key: string, cancellation: StreamCancellation): () => void {
    return this.local.register(key, cancellation);
  }

  async cancel(key: string): Promise<void> {
    await this.pool.query("SELECT pg_notify($1, $2)", [cancellationChannel, key]);
  }

  async close(): Promise<void> {
    const listener = this.listener;
    this.listener = undefined;
    if (listener !== undefined) {
      listener.removeListener("notification", this.onNotification);
      await listener.query(`UNLISTEN ${cancellationChannel}`);
      listener.release();
    }
    await this.local.close();
  }
}
