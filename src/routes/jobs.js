"use strict";

const { Router } = require("express");
const { LeaseJobScheduler } = require("../services/jobScheduler");

function createJobsRouter(scheduler = new LeaseJobScheduler()) {
  const router = Router();

  router.post("/", (req, res, next) => {
    try {
      const job = scheduler.enqueue(req.body || {});
      return res.status(202).json(job);
    } catch (err) {
      if (err.statusCode) return res.status(err.statusCode).json({ error: err.message });
      return next(err);
    }
  });

  router.post("/claim", (req, res, next) => {
    try {
      const job = scheduler.claimNext(req.body || {});
      return res.status(job ? 200 : 204).json(job || null);
    } catch (err) {
      if (err.statusCode) return res.status(err.statusCode).json({ error: err.message });
      return next(err);
    }
  });

  router.post("/:jobId/renew", (req, res, next) => {
    try {
      return res.status(200).json(scheduler.renewLease({ ...req.body, jobId: req.params.jobId }));
    } catch (err) {
      if (err.statusCode) return res.status(err.statusCode).json({ error: err.message });
      return next(err);
    }
  });

  router.post("/:jobId/complete", (req, res, next) => {
    try {
      return res.status(200).json(scheduler.complete({ ...req.body, jobId: req.params.jobId }));
    } catch (err) {
      if (err.statusCode) return res.status(err.statusCode).json({ error: err.message });
      return next(err);
    }
  });

  router.get("/metrics", (_req, res) => res.status(200).json(scheduler.snapshotMetrics()));
  router.get("/metrics.prom", (_req, res) => res.type("text/plain").send(`${scheduler.prometheusMetrics()}\n`));

  return router;
}

module.exports = { createJobsRouter };
