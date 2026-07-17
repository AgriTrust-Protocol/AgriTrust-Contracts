"use strict";

const { randomUUID } = require("crypto");

const SERVICE_NAME = process.env.OTEL_SERVICE_NAME || "agritrust-contracts-api";
const SERVICE_VERSION = process.env.npm_package_version || "1.0.0";
const DEPLOYMENT_ENVIRONMENT = process.env.NODE_ENV || "development";

function now() {
  return new Date().toISOString();
}

function baseRecord(severityText, body, attributes = {}) {
  return {
    timestamp: now(),
    severity_text: severityText,
    severity_number: severityNumber(severityText),
    body,
    resource: {
      "service.name": SERVICE_NAME,
      "service.version": SERVICE_VERSION,
      "deployment.environment.name": DEPLOYMENT_ENVIRONMENT,
    },
    attributes,
  };
}

function severityNumber(severityText) {
  switch (severityText) {
    case "DEBUG": return 5;
    case "INFO": return 9;
    case "WARN": return 13;
    case "ERROR": return 17;
    default: return 9;
  }
}

function write(record) {
  const line = JSON.stringify(record);
  if (record.severity_text === "ERROR") {
    console.error(line);
    return;
  }
  if (record.severity_text === "WARN") {
    console.warn(line);
    return;
  }
  console.log(line);
}

function log(severityText, body, attributes = {}) {
  write(baseRecord(severityText, body, attributes));
}

function info(body, attributes) {
  log("INFO", body, attributes);
}

function warn(body, attributes) {
  log("WARN", body, attributes);
}

function error(body, attributes) {
  log("ERROR", body, attributes);
}

function requestId() {
  return randomUUID();
}

function durationMs(startedAt) {
  const elapsed = process.hrtime.bigint() - startedAt;
  return Number(elapsed / 1000000n);
}

function httpRequestAttributes(req, res, startedAt) {
  const routePath = req.route?.path ? `${req.baseUrl || ""}${req.route.path}` : req.originalUrl;
  return {
    "http.request.method": req.method,
    "url.path": req.originalUrl.split("?")[0],
    "url.scheme": req.protocol,
    "url.query": req.originalUrl.includes("?") ? req.originalUrl.split("?").slice(1).join("?") : undefined,
    "http.route": routePath,
    "http.response.status_code": res.statusCode,
    "server.address": req.hostname,
    "user_agent.original": req.get("user-agent"),
    "client.address": req.ip,
    "network.protocol.version": req.httpVersion,
    "event.duration_ms": durationMs(startedAt),
    "request.id": req.id,
  };
}

function withoutUndefined(value) {
  return Object.fromEntries(Object.entries(value).filter(([, v]) => v !== undefined));
}

function requestLogger(req, res, next) {
  const startedAt = process.hrtime.bigint();
  req.id = req.get("x-request-id") || requestId();
  res.setHeader("x-request-id", req.id);

  res.on("finish", () => {
    const attributes = withoutUndefined(httpRequestAttributes(req, res, startedAt));
    const level = res.statusCode >= 500 ? "ERROR" : res.statusCode >= 400 ? "WARN" : "INFO";
    log(level, "http.server.request", attributes);
  });

  next();
}

module.exports = {
  error,
  info,
  log,
  requestLogger,
  warn,
};
