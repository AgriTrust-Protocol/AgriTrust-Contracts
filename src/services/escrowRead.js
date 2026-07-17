"use strict";

const { onChainAdapter } = require("../adapters/onChainAdapter");
const { buildKey, getOrSet } = require("./cache");

// ─── Validation ───────────────────────────────────────────────────────────────

const ESCROW_ID_RE = /^[a-zA-Z0-9_-]{1,64}$/;

/**
 * Validate an escrow ID string.
 * @param {string} id
 * @throws {Error} with `statusCode = 400` if invalid.
 */
function validateEscrowId(id) {
  if (typeof id !== "string" || !ESCROW_ID_RE.test(id)) {
    const err = new Error("Invalid escrow ID");
    err.statusCode = 400;
    throw err;
  }
}

// ─── Core read ────────────────────────────────────────────────────────────────

/**
 * Fetch and normalise escrow state for `escrowId`.
 *
 * @param {string} escrowId
 * @returns {Promise<{
 *   escrow_id:  string,
 *   balance:    string,
 *   recipient:  string,
 *   status:     string,
 *   legal_hold: boolean
 * }>}
 */
async function readEscrow(escrowId) {
  validateEscrowId(escrowId);

  let raw;
  try {
    raw = await getOrSet(
      buildKey("escrow", escrowId),
      () => onChainAdapter.getEscrow(escrowId)
    );
  } catch (err) {
    // Re-throw validation errors as-is; wrap everything else
    if (err.statusCode) throw err;
    const wrapped = new Error("Failed to fetch escrow data");
    wrapped.statusCode = 503;
    throw wrapped;
  }

  if (!raw || typeof raw !== "object") {
    const err = new Error("Escrow not found");
    err.statusCode = 404;
    throw err;
  }

  return normalise(escrowId, raw);
}

// ─── Normalisation ────────────────────────────────────────────────────────────

/**
 * Map raw on-chain data to the canonical escrow shape.
 * `legal_hold` defaults to `true` (safe-fail) when the field is missing or
 * not a boolean — this prevents a missing field from accidentally unblocking
 * a held escrow.
 *
 * @param {string} escrowId
 * @param {object} raw
 * @returns {object}
 */
function normalise(escrowId, raw) {
  const legalHold =
    typeof raw.legal_hold === "boolean"
      ? raw.legal_hold
      : raw.legalHold === true
        ? true
        : raw.legal_hold === false || raw.legalHold === false
          ? false
          : true; // safe default: treat unknown as held

  return {
    escrow_id:  escrowId,
    balance:    String(raw.balance   ?? "0"),
    recipient:  String(raw.recipient ?? ""),
    status:     String(raw.status    ?? "unknown"),
    legal_hold: legalHold,
  };
}

// ─── Exports ──────────────────────────────────────────────────────────────────

module.exports = { readEscrow, validateEscrowId, normalise };
