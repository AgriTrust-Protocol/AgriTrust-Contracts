"use strict";

const { Router } = require("express");
const { listRunbooks, snapshotIncidentMetrics, triggerIncident } = require("../services/incidentRunbook");

const router = Router();

router.get("/runbooks", (_req, res) => res.status(200).json({ runbooks: listRunbooks() }));
router.get("/metrics", (_req, res) => res.status(200).json(snapshotIncidentMetrics()));
router.post("/trigger", async (req, res, next) => {
  try {
    const result = await triggerIncident(req.body || {}, { dryRun: req.query.dryRun === "true" });
    return res.status(result.ok ? 202 : 502).json(result);
  } catch (err) {
    if (err.statusCode) return res.status(err.statusCode).json({ error: err.message });
    return next(err);
  }
});

module.exports = router;
