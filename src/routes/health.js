"use strict";

const { Router } = require("express");

function createHealthRouter(postgresPool) {
  const router = Router();

  router.get("/postgres", async (_req, res) => {
    const result = await postgresPool.probe();
    return res.status(result.ok ? 200 : 503).json(result);
  });

  router.get("/metrics", (_req, res) => {
    res.type("text/plain").send(`${postgresPool.prometheusMetrics()}\n`);
  });

  return router;
}

module.exports = { createHealthRouter };
