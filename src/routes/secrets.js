"use strict";

const { Router } = require("express");
const {
  InMemorySecretProvider,
  SecretRotationService,
  SECRET_TYPES,
} = require("../services/secretRotation");

const provider = new InMemorySecretProvider([
  {
    name: "database.primary",
    type: SECRET_TYPES.DATABASE,
    value: process.env.DATABASE_URL || "postgres://placeholder",
    version: 1,
    rotatedAt: new Date(0).toISOString(),
  },
  {
    name: "api.partner.default",
    type: SECRET_TYPES.API_KEY,
    value: process.env.PARTNER_API_KEY || "placeholder-api-key",
    version: 1,
    rotatedAt: new Date(0).toISOString(),
  },
]);

const rotationService = new SecretRotationService({ provider });
const router = Router();

router.get("/metrics", (_req, res) => {
  res.status(200).json(rotationService.metrics());
});

router.post("/rotate/:name", async (req, res, next) => {
  try {
    const rotated = await rotationService.rotateSecret(req.params.name);
    return res.status(202).json(rotated);
  } catch (err) {
    if (err.statusCode) {
      return res.status(err.statusCode).json({ error: err.message });
    }
    return next(err);
  }
});

module.exports = { router, rotationService };
