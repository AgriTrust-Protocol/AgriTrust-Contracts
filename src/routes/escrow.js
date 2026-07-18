/**
 * escrow.js — Express router
 * ──────────────────────────
 * Endpoints:
 *
 *   GET  /escrow/:escrowId          → read escrow state (includes legal_hold)
 *   POST /escrow/:escrowId/fund     → fund escrow (blocked if legal_hold)
 *   POST /escrow/:escrowId/release  → release escrow (blocked if legal_hold)
 *   POST /escrow/:escrowId/withdraw → withdraw from escrow (blocked if legal_hold)
 */

"use strict";

const { Router }      = require("express");
const { readEscrow }  = require("../services/escrowRead");
const legalHoldGate   = require("../middleware/legalHoldGate");
const { featureFlagGate } = require("../services/degradation");

const router = Router({ mergeParams: true });

// ─── GET /escrow/:escrowId ────────────────────────────────────────────────────

/**
 * @route   GET /escrow/:escrowId
 * @desc    Read escrow state.  Always returns `legal_hold` in the response.
 *          Clients MUST check `legal_hold` before initiating any funding action.
 * @access  Public
 */
router.get("/:escrowId", featureFlagGate("escrowRead"), async (req, res, next) => {
  try {
    const escrow = await readEscrow(req.params.escrowId);
    return res.status(200).json(escrow);
  } catch (err) {
    if (err.statusCode) {
      return res.status(err.statusCode).json({ error: err.message });
    }
    return next(err);
  }
});

// ─── POST /escrow/:escrowId/fund ──────────────────────────────────────────────

/**
 * @route   POST /escrow/:escrowId/fund
 * @desc    Fund an escrow.  Blocked with 502 if legal_hold is true.
 * @body    { amount: string }
 * @access  Authenticated (auth middleware omitted for brevity)
 */
router.post("/:escrowId/fund", featureFlagGate("mutationEndpoints"), legalHoldGate, async (req, res, next) => {
  try {
    const { amount } = req.body;
    if (!amount) {
      return res.status(400).json({ error: "Missing amount" });
    }
    // req.escrow is already populated by legalHoldGate
    // TODO: submit on-chain funding transaction
    return res.status(200).json({
      message:   "Funding initiated",
      escrow_id: req.escrow.escrow_id,
      amount,
    });
  } catch (err) {
    return next(err);
  }
});

// ─── POST /escrow/:escrowId/release ──────────────────────────────────────────

/**
 * @route   POST /escrow/:escrowId/release
 * @desc    Release escrow funds to recipient.  Blocked with 502 if legal_hold.
 * @access  Authenticated
 */
router.post("/:escrowId/release", featureFlagGate("mutationEndpoints"), legalHoldGate, async (req, res, next) => {
  try {
    // TODO: submit on-chain release transaction
    return res.status(200).json({
      message:   "Release initiated",
      escrow_id: req.escrow.escrow_id,
    });
  } catch (err) {
    return next(err);
  }
});

// ─── POST /escrow/:escrowId/withdraw ─────────────────────────────────────────

/**
 * @route   POST /escrow/:escrowId/withdraw
 * @desc    Withdraw from escrow.  Blocked with 502 if legal_hold.
 * @access  Authenticated
 */
router.post("/:escrowId/withdraw", featureFlagGate("mutationEndpoints"), legalHoldGate, async (req, res, next) => {
  try {
    // TODO: submit on-chain withdrawal transaction
    return res.status(200).json({
      message:   "Withdrawal initiated",
      escrow_id: req.escrow.escrow_id,
    });
  } catch (err) {
    return next(err);
  }
});

module.exports = router;
