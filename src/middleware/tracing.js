/**
 * OpenTelemetry-compatible HTTP tracing middleware.
 *
 * This module implements W3C Trace Context propagation (`traceparent` and
 * `tracestate`) without requiring the optional OpenTelemetry packages at
 * runtime. The emitted JSON events use OpenTelemetry semantic attribute names
 * so they can be shipped to an OTLP collector by the platform log pipeline.
 */

"use strict";

const crypto = require("crypto");

const TRACEPARENT_RE = /^([\da-f]{2})-([\da-f]{32})-([\da-f]{16})-([\da-f]{2})$/;
const ZERO_TRACE_ID = "00000000000000000000000000000000";
const ZERO_SPAN_ID = "0000000000000000";

function randomHex(bytes) {
  return crypto.randomBytes(bytes).toString("hex");
}

function isValidTraceId(traceId) {
  return typeof traceId === "string" && traceId.length === 32 && traceId !== ZERO_TRACE_ID;
}

function isValidSpanId(spanId) {
  return typeof spanId === "string" && spanId.length === 16 && spanId !== ZERO_SPAN_ID;
}

function parseTraceparent(header) {
  if (typeof header !== "string") return null;

  const match = header.trim().match(TRACEPARENT_RE);
  if (!match) return null;

  const [, version, traceId, parentSpanId, traceFlags] = match;
  if (version === "ff" || !isValidTraceId(traceId) || !isValidSpanId(parentSpanId)) {
    return null;
  }

  return { version, traceId, parentSpanId, traceFlags };
}

function buildTraceparent({ traceId, spanId, traceFlags = "01" }) {
  return `00-${traceId}-${spanId}-${traceFlags}`;
}

function getHeader(req, name) {
  if (typeof req.get === "function") return req.get(name);
  return req.headers?.[name.toLowerCase()];
}

function createTraceContext(req) {
  const inbound = parseTraceparent(getHeader(req, "traceparent"));
  const spanId = randomHex(8);
  const traceId = inbound?.traceId || randomHex(16);

  return {
    traceId,
    spanId,
    parentSpanId: inbound?.parentSpanId || null,
    traceFlags: inbound?.traceFlags || "01",
    traceparent: buildTraceparent({
      traceId,
      spanId,
      traceFlags: inbound?.traceFlags || "01",
    }),
    tracestate: getHeader(req, "tracestate") || undefined,
    sampled: ((parseInt(inbound?.traceFlags || "01", 16) & 1) === 1),
  };
}

function logSpan(event, payload, logger = console) {
  logger.log(JSON.stringify({ event, ...payload }));
}

function tracingMiddleware(options = {}) {
  const serviceName = options.serviceName || process.env.OTEL_SERVICE_NAME || "agritrust-contracts-api";
  const logger = options.logger || console;
  const enabled = options.enabled ?? process.env.OTEL_TRACES_ENABLED !== "false";

  return (req, res, next) => {
    const startedAt = process.hrtime.bigint();
    const context = createTraceContext(req);
    req.traceContext = context;

    res.setHeader("traceparent", context.traceparent);
    if (context.tracestate) res.setHeader("tracestate", context.tracestate);

    res.on("finish", () => {
      if (!enabled) return;
      const durationNs = Number(process.hrtime.bigint() - startedAt);
      logSpan("otel.http.server.span", {
        trace_id: context.traceId,
        span_id: context.spanId,
        parent_span_id: context.parentSpanId,
        service_name: serviceName,
        name: `${req.method} ${req.route?.path || req.path}`,
        kind: "SERVER",
        duration_ms: Number((durationNs / 1e6).toFixed(3)),
        status_code: res.statusCode,
        attributes: {
          "http.request.method": req.method,
          "http.route": req.route?.path || req.path,
          "http.response.status_code": res.statusCode,
          "url.path": req.originalUrl || req.path,
          "user_agent.original": getHeader(req, "user-agent") || "",
        },
      }, logger);
    });

    return next();
  };
}

module.exports = {
  buildTraceparent,
  createTraceContext,
  parseTraceparent,
  tracingMiddleware,
};
