/**
 * Grant Stream API — Express entry point
 *
 * Registers all routes and starts the HTTP server.
 * Kept minimal so tests can import `app` without binding a port.
 */

"use strict";

const express = require("express");
const escrowRoutes = require("./routes/escrow");
const webhookRoutes = require("./routes/webhooks");

const app = express();

app.use(express.json());

// ── Routes ────────────────────────────────────────────────────────────────────
app.use("/escrow", escrowRoutes);
app.use("/webhooks", webhookRoutes);

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
