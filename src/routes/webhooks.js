"use strict";

const { Router } = require("express");
const { snapshotMetrics, verifySignature, webhookDeadLetterQueue } = require("../services/webhookDelivery");

const router = Router();

router.get("/metrics", (_req, res) => {
  res.status(200).json(snapshotMetrics());
});

router.get("/dead-letter", (req, res) => {
  res.status(200).json({ entries: webhookDeadLetterQueue.list({ service: req.query.service }) });
});

router.post("/verify", (req, res) => {
  const secret = process.env.WEBHOOK_VERIFICATION_SECRET;
  if (!secret) return res.status(503).json({ error: "Webhook verification secret is not configured" });
  const ok = verifySignature({
    secret,
    payload: req.body,
    timestamp: req.get("x-agritrust-webhook-timestamp"),
    signature: req.get("x-agritrust-webhook-signature"),
  });
  return res.status(ok ? 200 : 401).json({ verified: ok });
});

module.exports = router;
