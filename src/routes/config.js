"use strict";

const { Router } = require("express");

function createConfigRouter(configManager) {
  const router = Router();

  router.get("/runtime", (_req, res) => {
    res.status(200).json({ config: configManager.current(), metrics: configManager.metrics() });
  });

  router.post("/reload", (_req, res) => {
    const result = configManager.reload();
    res.status(result.ok ? 202 : 422).json(result.ok ? { reloaded: true, config: result.config } : result);
  });

  return router;
}

module.exports = { createConfigRouter };
