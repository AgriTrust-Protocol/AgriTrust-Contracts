"use strict";

const { Router } = require("express");
const { createDefaultConfigAuditor, loadRuntimeConfig } = require("../services/configAudit");

function createConfigAuditRouter(auditor = createDefaultConfigAuditor()) {
  const router = Router();

  router.get("/snapshot", (_req, res) => {
    const result = auditor.audit(loadRuntimeConfig(), { service: "agritrust-contracts-api" });
    res.status(result.drift_detected ? 409 : 200).json(result);
  });

  router.get("/metrics", (_req, res) => {
    res.status(200).json(auditor.metrics());
  });

  return router;
}

module.exports = { createConfigAuditRouter };
