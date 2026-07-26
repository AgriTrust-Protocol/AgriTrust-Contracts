"use strict";

const { InMemoryDeadLetterQueue } = require("../src/services/deadLetterQueue");

describe("dead letter queue", () => {
  it("stores failed messages with deterministic ids and redacts secrets", () => {
    const dlq = new InMemoryDeadLetterQueue({ retentionMs: 1000 });
    const entry = dlq.enqueue({
      service: "worker-a",
      messageId: "msg-1",
      payload: { amount: 100, secret: "do-not-store" },
      reason: "handler_failed",
      error: new Error("downstream unavailable"),
      now: 0,
    });

    expect(entry.id).toMatch(/^[a-f0-9]{64}$/);
    expect(entry.payload.secret).toBe("[REDACTED]");
    expect(dlq.get(entry.id)).toMatchObject({ service: "worker-a", messageId: "msg-1" });
    expect(dlq.snapshotMetrics()).toMatchObject({ enqueued: 1, pending: 1, totalStored: 1 });
  });

  it("replays a pending message once and tracks replay metrics", async () => {
    const dlq = new InMemoryDeadLetterQueue();
    const entry = dlq.enqueue({ service: "worker-a", messageId: "msg-1", payload: { ok: true } });
    const handler = jest.fn().mockResolvedValue(undefined);

    await expect(dlq.replay(entry.id, handler, { now: 10 })).resolves.toMatchObject({ ok: true });
    await expect(dlq.replay(entry.id, handler)).resolves.toEqual({ ok: false, error: "already_replayed" });
    expect(handler).toHaveBeenCalledTimes(1);
    expect(dlq.snapshotMetrics()).toMatchObject({ replayed: 1, pending: 0, totalStored: 1 });
  });

  it("purges expired messages", () => {
    const dlq = new InMemoryDeadLetterQueue({ retentionMs: 5 });
    dlq.enqueue({ service: "worker-a", messageId: "msg-1", payload: {}, now: 10 });
    expect(dlq.purgeExpired({ now: 16 })).toBe(1);
    expect(dlq.snapshotMetrics()).toMatchObject({ purged: 1, pending: 0, totalStored: 0 });
  });
});
