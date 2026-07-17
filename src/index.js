/**
 * Grant Stream API — Express entry point
 *
 * Registers all routes and starts the HTTP server.
 * Kept minimal so tests can import `app` without binding a port.
 */

"use strict";

const express = require("express");
const escrowRoutes = require("./routes/escrow");
const logger = require("./observability/logger");

const app = express();

app.use(express.json());
app.use(logger.requestLogger);

// ── Routes ────────────────────────────────────────────────────────────────────
app.use("/escrow", escrowRoutes);

// ── 404 catch-all ─────────────────────────────────────────────────────────────
app.use((_req, res) => {
  res.status(404).json({ error: "Not found" });
});

// ── Global error handler ──────────────────────────────────────────────────────
// eslint-disable-next-line no-unused-vars
app.use((err, _req, res, _next) => {
  // Never leak stack traces to clients
  logger.error("http.server.error", {
    "error.type": err.name || "Error",
    "error.message": err.message,
  });
  res.status(500).json({ error: "Internal server error" });
});

// ── Start (only when run directly) ───────────────────────────────────────────
if (require.main === module) {
  const PORT = process.env.PORT || 3000;
  app.listen(PORT, () => {
    logger.info("service.started", { "server.port": Number(PORT) });
  });
}

module.exports = app;
