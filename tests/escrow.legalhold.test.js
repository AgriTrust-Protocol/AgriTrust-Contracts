/**
 * escrow.legalhold.test.js
 * ────────────────────────
 * Jest + Supertest tests for:
 *  - escrowRead service (normalisation, validation, safe defaults)
 *  - legalHoldGate middleware
 *  - GET /escrow/:escrowId  (read endpoint)
 *  - POST /escrow/:escrowId/fund    (gated)
 *  - POST /escrow/:escrowId/release (gated)
 *  - POST /escrow/:escrowId/withdraw (gated)
 *
 * The on-chain adapter is always mocked — no network calls.
 */

"use strict";

const request = require("supertest");

// ─── Mock the adapter BEFORE requiring any app code ──────────────────────────
jest.mock("../src/adapters/onChainAdapter");
const { onChainAdapter } = require("../src/adapters/onChainAdapter");

const app = require("../src/index");
const { readEscrow, normalise, validateEscrowId } = require("../src/services/escrowRead");
const { getDegradationSnapshot, resetDegradation } = require("../src/services/degradation");

beforeEach(() => resetDegradation());

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const ESCROW_ID = "escrow-abc-123";

const baseRaw = {
  balance:    "5000000000000000000", // 5 ETH in wei
  recipient:  "0xRecipientAddress",
  status:     "active",
  legal_hold: false,
};

const heldRaw = { ...baseRaw, legal_hold: true };

// ─── Helper ───────────────────────────────────────────────────────────────────

function mockGetEscrow(raw) {
  onChainAdapter.getEscrow.mockResolvedValue(raw);
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. escrowRead service — unit tests
// ─────────────────────────────────────────────────────────────────────────────

describe("escrowRead service", () => {
  beforeEach(() => jest.clearAllMocks());

  // ── 1a. Normalisation ──────────────────────────────────────────────────────

  describe("normalise()", () => {
    it("maps legal_hold: false correctly", () => {
      const result = normalise(ESCROW_ID, { ...baseRaw, legal_hold: false });
      expect(result.legal_hold).toBe(false);
    });

    it("maps legal_hold: true correctly", () => {
      const result = normalise(ESCROW_ID, { ...baseRaw, legal_hold: true });
      expect(result.legal_hold).toBe(true);
    });

    it("maps camelCase legalHold: true correctly", () => {
      const raw = { ...baseRaw, legal_hold: undefined, legalHold: true };
      const result = normalise(ESCROW_ID, raw);
      expect(result.legal_hold).toBe(true);
    });

    it("maps camelCase legalHold: false correctly", () => {
      const raw = { ...baseRaw, legal_hold: undefined, legalHold: false };
      const result = normalise(ESCROW_ID, raw);
      expect(result.legal_hold).toBe(false);
    });

    it("defaults legal_hold to TRUE when field is missing (safe-fail)", () => {
      const raw = { balance: "0", recipient: "0xABC", status: "active" };
      const result = normalise(ESCROW_ID, raw);
      expect(result.legal_hold).toBe(true);
    });

    it("defaults legal_hold to TRUE when field is null", () => {
      const result = normalise(ESCROW_ID, { ...baseRaw, legal_hold: null });
      expect(result.legal_hold).toBe(true);
    });

    it("defaults legal_hold to TRUE when field is a string", () => {
      const result = normalise(ESCROW_ID, { ...baseRaw, legal_hold: "false" });
      expect(result.legal_hold).toBe(true);
    });

    it("includes all required fields in output", () => {
      const result = normalise(ESCROW_ID, baseRaw);
      expect(result).toMatchObject({
        escrow_id:  ESCROW_ID,
        balance:    baseRaw.balance,
        recipient:  baseRaw.recipient,
        status:     baseRaw.status,
        legal_hold: false,
      });
    });

    it("coerces missing balance to '0'", () => {
      const result = normalise(ESCROW_ID, { recipient: "0xABC", status: "active", legal_hold: false });
      expect(result.balance).toBe("0");
    });
  });

  // ── 1b. validateEscrowId ───────────────────────────────────────────────────

  describe("validateEscrowId()", () => {
    it("accepts valid alphanumeric IDs", () => {
      expect(() => validateEscrowId("escrow-123")).not.toThrow();
      expect(() => validateEscrowId("ABC_xyz-001")).not.toThrow();
    });

    it("throws 400 for empty string", () => {
      expect(() => validateEscrowId("")).toThrow(expect.objectContaining({ statusCode: 400 }));
    });

    it("throws 400 for non-string input", () => {
      expect(() => validateEscrowId(123)).toThrow(expect.objectContaining({ statusCode: 400 }));
      expect(() => validateEscrowId(null)).toThrow(expect.objectContaining({ statusCode: 400 }));
      expect(() => validateEscrowId(undefined)).toThrow(expect.objectContaining({ statusCode: 400 }));
    });

    it("throws 400 for ID with special characters", () => {
      expect(() => validateEscrowId("escrow<script>")).toThrow(
        expect.objectContaining({ statusCode: 400 })
      );
    });

    it("throws 400 for ID longer than 64 chars", () => {
      expect(() => validateEscrowId("a".repeat(65))).toThrow(
        expect.objectContaining({ statusCode: 400 })
      );
    });
  });

  // ── 1c. readEscrow ─────────────────────────────────────────────────────────

  describe("readEscrow()", () => {
    it("returns normalised escrow with legal_hold: false", async () => {
      mockGetEscrow(baseRaw);
      const result = await readEscrow(ESCROW_ID);
      expect(result.legal_hold).toBe(false);
      expect(result.escrow_id).toBe(ESCROW_ID);
    });

    it("returns normalised escrow with legal_hold: true", async () => {
      mockGetEscrow(heldRaw);
      const result = await readEscrow(ESCROW_ID);
      expect(result.legal_hold).toBe(true);
    });

    it("throws 404 when adapter returns null", async () => {
      onChainAdapter.getEscrow.mockResolvedValue(null);
      await expect(readEscrow(ESCROW_ID)).rejects.toMatchObject({ statusCode: 404 });
    });

    it("throws 503 when adapter throws a generic error", async () => {
      onChainAdapter.getEscrow.mockRejectedValue(new Error("RPC timeout"));
      await expect(readEscrow(ESCROW_ID)).rejects.toMatchObject({ statusCode: 503 });
    });

    it("re-throws 400 validation errors from adapter", async () => {
      const validationErr = new Error("bad id");
      validationErr.statusCode = 400;
      onChainAdapter.getEscrow.mockRejectedValue(validationErr);
      await expect(readEscrow(ESCROW_ID)).rejects.toMatchObject({ statusCode: 400 });
    });

    it("throws 400 for invalid escrow ID", async () => {
      await expect(readEscrow("bad id!")).rejects.toMatchObject({ statusCode: 400 });
      expect(onChainAdapter.getEscrow).not.toHaveBeenCalled();
    });
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 2. GET /escrow/:escrowId — read endpoint
// ─────────────────────────────────────────────────────────────────────────────

describe("GET /escrow/:escrowId", () => {
  beforeEach(() => jest.clearAllMocks());

  it("returns 200 with legal_hold: false when not held", async () => {
    mockGetEscrow(baseRaw);
    const res = await request(app).get(`/escrow/${ESCROW_ID}`);
    expect(res.status).toBe(200);
    expect(res.body.legal_hold).toBe(false);
    expect(res.body.escrow_id).toBe(ESCROW_ID);
  });

  it("returns 200 with legal_hold: true when held", async () => {
    mockGetEscrow(heldRaw);
    const res = await request(app).get(`/escrow/${ESCROW_ID}`);
    expect(res.status).toBe(200);
    expect(res.body.legal_hold).toBe(true);
  });

  it("includes all required fields in response", async () => {
    mockGetEscrow(baseRaw);
    const res = await request(app).get(`/escrow/${ESCROW_ID}`);
    expect(res.body).toHaveProperty("escrow_id");
    expect(res.body).toHaveProperty("balance");
    expect(res.body).toHaveProperty("recipient");
    expect(res.body).toHaveProperty("status");
    expect(res.body).toHaveProperty("legal_hold");
  });

  it("returns 404 when escrow not found", async () => {
    onChainAdapter.getEscrow.mockResolvedValue(null);
    const res = await request(app).get(`/escrow/${ESCROW_ID}`);
    expect(res.status).toBe(404);
    expect(res.body).toHaveProperty("error");
  });

  it("returns 400 for invalid escrow ID", async () => {
    const res = await request(app).get("/escrow/bad id!");
    expect(res.status).toBe(400);
  });

  it("returns 503 when adapter is unavailable", async () => {
    onChainAdapter.getEscrow.mockRejectedValue(new Error("network error"));
    const res = await request(app).get(`/escrow/${ESCROW_ID}`);
    expect(res.status).toBe(503);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 3. POST /escrow/:escrowId/fund — legal-hold gating
// ─────────────────────────────────────────────────────────────────────────────

describe("POST /escrow/:escrowId/fund", () => {
  beforeEach(() => jest.clearAllMocks());

  it("proceeds (200) when legal_hold is false", async () => {
    mockGetEscrow(baseRaw);
    const res = await request(app)
      .post(`/escrow/${ESCROW_ID}/fund`)
      .send({ amount: "1000000000000000000" });
    expect(res.status).toBe(200);
    expect(res.body.message).toBe("Funding initiated");
  });

  it("returns 502 when legal_hold is true", async () => {
    mockGetEscrow(heldRaw);
    const res = await request(app)
      .post(`/escrow/${ESCROW_ID}/fund`)
      .send({ amount: "1000000000000000000" });
    expect(res.status).toBe(502);
    expect(res.body.error).toBe("Escrow is under legal hold");
  });

  it("does NOT call downstream logic when legal_hold is true", async () => {
    mockGetEscrow(heldRaw);
    const res = await request(app)
      .post(`/escrow/${ESCROW_ID}/fund`)
      .send({ amount: "1000000000000000000" });
    expect(res.status).toBe(502);
    // Adapter was called once (for the gate check), but no funding tx
    expect(onChainAdapter.getEscrow).toHaveBeenCalledTimes(1);
  });

  it("returns 400 when amount is missing", async () => {
    mockGetEscrow(baseRaw);
    const res = await request(app)
      .post(`/escrow/${ESCROW_ID}/fund`)
      .send({});
    expect(res.status).toBe(400);
    expect(res.body.error).toBe("Missing amount");
  });

  it("returns 400 for invalid escrow ID", async () => {
    const res = await request(app)
      .post("/escrow/bad id!/fund")
      .send({ amount: "100" });
    expect(res.status).toBe(400);
  });

  it("returns 502 when adapter is unavailable (safe-fail)", async () => {
    onChainAdapter.getEscrow.mockRejectedValue(new Error("RPC down"));
    const res = await request(app)
      .post(`/escrow/${ESCROW_ID}/fund`)
      .send({ amount: "100" });
    expect(res.status).toBe(502);
    expect(res.body.error).toBe("Escrow is under legal hold");
  });

  it("returns 404 when escrow not found", async () => {
    onChainAdapter.getEscrow.mockResolvedValue(null);
    const res = await request(app)
      .post(`/escrow/${ESCROW_ID}/fund`)
      .send({ amount: "100" });
    expect(res.status).toBe(404);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 4. POST /escrow/:escrowId/release — legal-hold gating
// ─────────────────────────────────────────────────────────────────────────────

describe("POST /escrow/:escrowId/release", () => {
  beforeEach(() => jest.clearAllMocks());

  it("proceeds (200) when legal_hold is false", async () => {
    mockGetEscrow(baseRaw);
    const res = await request(app).post(`/escrow/${ESCROW_ID}/release`).send({});
    expect(res.status).toBe(200);
    expect(res.body.message).toBe("Release initiated");
  });

  it("returns 502 when legal_hold is true", async () => {
    mockGetEscrow(heldRaw);
    const res = await request(app).post(`/escrow/${ESCROW_ID}/release`).send({});
    expect(res.status).toBe(502);
    expect(res.body.error).toBe("Escrow is under legal hold");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 5. POST /escrow/:escrowId/withdraw — legal-hold gating
// ─────────────────────────────────────────────────────────────────────────────

describe("POST /escrow/:escrowId/withdraw", () => {
  beforeEach(() => jest.clearAllMocks());

  it("proceeds (200) when legal_hold is false", async () => {
    mockGetEscrow(baseRaw);
    const res = await request(app).post(`/escrow/${ESCROW_ID}/withdraw`).send({});
    expect(res.status).toBe(200);
    expect(res.body.message).toBe("Withdrawal initiated");
  });

  it("returns 502 when legal_hold is true", async () => {
    mockGetEscrow(heldRaw);
    const res = await request(app).post(`/escrow/${ESCROW_ID}/withdraw`).send({});
    expect(res.status).toBe(502);
    expect(res.body.error).toBe("Escrow is under legal hold");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 6. legalHoldGate middleware — unit tests
// ─────────────────────────────────────────────────────────────────────────────

describe("legalHoldGate middleware", () => {
  const legalHoldGate = require("../src/middleware/legalHoldGate");

  function makeReqRes(params = {}, body = {}, query = {}) {
    const req = { params, body, query };
    const res = {
      _status: null,
      _body:   null,
      status(code) { this._status = code; return this; },
      json(data)   { this._body   = data; return this; },
    };
    const next = jest.fn();
    return { req, res, next };
  }

  beforeEach(() => jest.clearAllMocks());

  it("calls next() and attaches req.escrow when not held", async () => {
    mockGetEscrow(baseRaw);
    const { req, res, next } = makeReqRes({ escrowId: ESCROW_ID });
    await legalHoldGate(req, res, next);
    expect(next).toHaveBeenCalled();
    expect(req.escrow).toBeDefined();
    expect(req.escrow.legal_hold).toBe(false);
  });

  it("returns 502 and does NOT call next() when held", async () => {
    mockGetEscrow(heldRaw);
    const { req, res, next } = makeReqRes({ escrowId: ESCROW_ID });
    await legalHoldGate(req, res, next);
    expect(next).not.toHaveBeenCalled();
    expect(res._status).toBe(502);
    expect(res._body.error).toBe("Escrow is under legal hold");
  });

  it("returns 400 when escrowId is missing", async () => {
    const { req, res, next } = makeReqRes({}, {}, {});
    await legalHoldGate(req, res, next);
    expect(res._status).toBe(400);
    expect(next).not.toHaveBeenCalled();
  });

  it("resolves escrowId from req.body.escrow_id", async () => {
    mockGetEscrow(baseRaw);
    const { req, res, next } = makeReqRes({}, { escrow_id: ESCROW_ID });
    await legalHoldGate(req, res, next);
    expect(next).toHaveBeenCalled();
  });

  it("resolves escrowId from req.query.escrow_id", async () => {
    mockGetEscrow(baseRaw);
    const { req, res, next } = makeReqRes({}, {}, { escrow_id: ESCROW_ID });
    await legalHoldGate(req, res, next);
    expect(next).toHaveBeenCalled();
  });

  it("returns 502 (safe-fail) when adapter throws", async () => {
    onChainAdapter.getEscrow.mockRejectedValue(new Error("network error"));
    const { req, res, next } = makeReqRes({ escrowId: ESCROW_ID });
    await legalHoldGate(req, res, next);
    expect(res._status).toBe(502);
    expect(next).not.toHaveBeenCalled();
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 7. Edge cases
// ─────────────────────────────────────────────────────────────────────────────

describe("Edge cases", () => {
  beforeEach(() => jest.clearAllMocks());

  it("legal_hold defaults to true when on-chain field is missing", async () => {
    mockGetEscrow({ balance: "0", recipient: "0xABC", status: "active" });
    const res = await request(app).get(`/escrow/${ESCROW_ID}`);
    expect(res.status).toBe(200);
    expect(res.body.legal_hold).toBe(true);
  });

  it("funding is blocked when legal_hold defaults to true (missing field)", async () => {
    mockGetEscrow({ balance: "0", recipient: "0xABC", status: "active" });
    const res = await request(app)
      .post(`/escrow/${ESCROW_ID}/fund`)
      .send({ amount: "100" });
    expect(res.status).toBe(502);
  });

  it("returns 404 for unknown routes", async () => {
    const res = await request(app).get("/unknown-route");
    expect(res.status).toBe(404);
  });

  it("handles escrow with zero balance correctly", async () => {
    mockGetEscrow({ ...baseRaw, balance: "0", legal_hold: false });
    const res = await request(app).get(`/escrow/${ESCROW_ID}`);
    expect(res.status).toBe(200);
    expect(res.body.balance).toBe("0");
    expect(res.body.legal_hold).toBe(false);
  });
});


// ─────────────────────────────────────────────────────────────────────────────
// 8. Graceful degradation — feature flags and capacity shedding
// ─────────────────────────────────────────────────────────────────────────────

describe("Graceful degradation controls", () => {
  beforeEach(() => jest.clearAllMocks());

  it("returns 503 before escrow mutation work when mutation feature flag is off", async () => {
    resetDegradation({ flags: { mutationEndpoints: false } });
    const res = await request(app)
      .post(`/escrow/${ESCROW_ID}/fund`)
      .send({ amount: "100" });

    expect(res.status).toBe(503);
    expect(res.body).toEqual({
      error: "Feature temporarily disabled",
      feature: "mutationEndpoints",
    });
    expect(onChainAdapter.getEscrow).not.toHaveBeenCalled();
  });

  it("returns 503 before escrow reads when read feature flag is off", async () => {
    resetDegradation({ flags: { escrowRead: false } });
    const res = await request(app).get(`/escrow/${ESCROW_ID}`);

    expect(res.status).toBe(503);
    expect(res.body.feature).toBe("escrowRead");
    expect(onChainAdapter.getEscrow).not.toHaveBeenCalled();
  });

  it("sheds capacity with Retry-After when in-flight requests exceed the configured ceiling", async () => {
    resetDegradation({ flags: { shedCapacity: true }, maxInFlight: 1 });
    let releaseFirstRequest;
    onChainAdapter.getEscrow.mockImplementationOnce(
      () => new Promise((resolve) => {
        releaseFirstRequest = () => resolve(baseRaw);
      })
    );

    const first = request(app).get(`/escrow/${ESCROW_ID}`);
    const firstResponse = first.then((response) => response);
    await new Promise((resolve) => setImmediate(resolve));

    const second = await request(app).get(`/escrow/${ESCROW_ID}`);
    expect(second.status).toBe(503);
    expect(second.headers["retry-after"]).toBe("1");
    expect(second.body.error).toBe("Capacity temporarily unavailable");

    releaseFirstRequest();
    const completedFirst = await firstResponse;
    expect(completedFirst.status).toBe(200);
  });

  it("exposes degradation telemetry for monitoring and dashboards", async () => {
    resetDegradation({ flags: { shedCapacity: true }, maxInFlight: 3 });
    const res = await request(app).get("/ops/degradation");

    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      flags: { shedCapacity: true, mutationEndpoints: true, escrowRead: true },
      capacity: { in_flight: 0, max_in_flight: 3, shedding_enabled: true },
      counters: { accepted: 0, completed: 0, shed: 0, disabled: 0 },
    });
  });

  it("records disabled and shed counters in snapshots", async () => {
    resetDegradation({ flags: { mutationEndpoints: false } });
    await request(app).post(`/escrow/${ESCROW_ID}/release`).send({});
    const snapshot = getDegradationSnapshot();

    expect(snapshot.counters.disabled).toBe(1);
    expect(snapshot.counters.shed).toBe(0);
  });
});
