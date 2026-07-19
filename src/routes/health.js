"use strict";

const { Router } = require("express");

function createHealthRouter(postgresPool, capacityPlanner) {
  const router = Router();

  router.get("/postgres", async (_req, res) => {
    const result = await postgresPool.probe();
    return res.status(result.ok ? 200 : 503).json(result);
  });

  router.get("/capacity", (_req, res) => {
    res.status(200).json(capacityPlanner.snapshot());
  });

  router.get("/metrics", (_req, res) => {
    const capacityMetrics = capacityPlanner ? capacityPlanner.prometheusMetrics() : "";
    res.type("text/plain").send(`${postgresPool.prometheusMetrics()}\n${capacityMetrics}\n`);
  });

  return router;
}

module.exports = { createHealthRouter };
