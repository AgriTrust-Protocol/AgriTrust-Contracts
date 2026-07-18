/**
 * Grant Stream API — Express entry point
 *
 * Registers all routes and starts the HTTP server.
 * Kept minimal so tests can import `app` without binding a port.
 */

"use strict";

const express = require("express");
const escrowRoutes = require("./routes/escrow");
const { capacityShedding, getDegradationSnapshot } = require("./services/degradation");
const { createHealthRouter } = require("./routes/health");
const { buildDefaultPool } = require("./services/postgresPoolHealth");
const { tracingMiddleware } = require("./middleware/tracing");

const app = express();

app.use(tracingMiddleware());
app.use(express.json());
app.use(capacityShedding);

app.get("/healthz", (_req, res) => {
  res.status(200).json({ status: "ok" });
});

app.get("/ops/degradation", (_req, res) => {
  res.status(200).json(getDegradationSnapshot());
});

// Apply a system-wide per-tenant token bucket before all service routes.
app.use(createTenantRateLimiter());

// ── Routes ────────────────────────────────────────────────────────────────────
app.use("/escrow", escrowRoutes);
app.use("/internal/secrets", secretRoutes);
app.use("/health", createHealthRouter(buildDefaultPool()));

// ── 404 catch-all ─────────────────────────────────────────────────────────────
app.use((_req, res) => {
  res.status(404).json({ error: "Not found" });
});

// ── Global error handler ──────────────────────────────────────────────────────
// eslint-disable-next-line no-unused-vars
app.use((err, _req, res, _next) => {
  // Never leak stack traces to clients
  console.error("[error]", err.message);
  res.status(500).json({ error: "Internal server error" });
});

// ── Start (only when run directly) ───────────────────────────────────────────
if (require.main === module) {
  const PORT = process.env.PORT || 3000;
  app.listen(PORT, () => {
    console.log(`Grant Stream API listening on port ${PORT}`);
  });
}

module.exports = app;
