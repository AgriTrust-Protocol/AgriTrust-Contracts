"use strict";

const crypto = require("crypto");

const OTEL_SEVERITY_NUMBER = {
  TRACE: 1,
  DEBUG: 5,
  INFO: 9,
  WARN: 13,
  ERROR: 17,
  FATAL: 21,
};

const LOG_LEVELS = {
  trace: "TRACE",
  debug: "DEBUG",
  info: "INFO",
  warn: "WARN",
  error: "ERROR",
  fatal: "FATAL",
};

let _appName = process.env.OTEL_SERVICE_NAME || "agritrust-contracts-api";
let _appVersion = process.env.APP_VERSION || "1.0.0";
let _environment = process.env.NODE_ENV || "development";
let _minLevel = LOG_LEVELS[process.env.OTEL_LOG_LEVEL] || LOG_LEVELS.info;
let _instanceId = crypto.randomUUID();

function setServiceName(name) {
  _appName = name;
}

function setAppVersion(version) {
  _appVersion = version;
}

function setEnvironment(env) {
  _environment = env;
}

function setLogLevel(level) {
  if (LOG_LEVELS[level]) _minLevel = LOG_LEVELS[level];
}

function resetInstanceId() {
  _instanceId = crypto.randomUUID();
}

function shouldLog(level) {
  return OTEL_SEVERITY_NUMBER[level] >= OTEL_SEVERITY_NUMBER[_minLevel];
}

function severityNumber(level) {
  return OTEL_SEVERITY_NUMBER[level] || 0;
}

function nowISO() {
  return new Date().toISOString();
}

function truncate(value, maxLen = 4096) {
  const s = String(value ?? "");
  return s.length > maxLen ? s.slice(0, maxLen) + "..." : s;
}

function serializeAttributes(attrs) {
  if (!attrs || typeof attrs !== "object") return attrs;
  const result = {};
  for (const [key, val] of Object.entries(attrs)) {
    if (val === null || val === undefined) continue;
    if (typeof val === "object") {
      try {
        result[key] = JSON.stringify(val);
      } catch {
        result[key] = String(val);
      }
    } else {
      result[key] = val;
    }
  }
  return result;
}

function createLogEntry({ severityText, body, traceId, spanId, traceFlags, resource, attributes, exception }) {
  const entry = {
    timestamp: nowISO(),
    severity_text: severityText,
    severity_number: severityNumber(severityText),
    service_name: _appName,
    service_version: _appVersion,
    environment: _environment,
    instance_id: _instanceId,
  };

  if (traceId) entry.trace_id = traceId;
  if (spanId) entry.span_id = spanId;
  if (traceFlags) entry.trace_flags = traceFlags;

  if (resource) entry.resource = resource;
  if (body !== undefined) entry.body = truncate(body);
  if (attributes) entry.attributes = serializeAttributes(attributes);

  if (exception) {
    entry.attributes = {
      ...(entry.attributes || {}),
      "exception.type": exception.type || typeof exception,
      "exception.message": truncate(exception.message || String(exception), 2048),
      "exception.stacktrace": truncate(exception.stack || "", 8192),
      "exception.escaped": exception.escaped ?? false,
    };
  }

  return entry;
}

class StructuredLogger {
  constructor(options = {}) {
    this._writer = options.writer || ((entry) => process.stdout.write(JSON.stringify(entry) + "\n"));
    this._errorWriter = options.errorWriter || ((entry) => process.stderr.write(JSON.stringify(entry) + "\n"));
    this._minLevel = options.level ? (LOG_LEVELS[options.level] || _minLevel) : _minLevel;
    this._ctx = { traceId: null, spanId: null, traceFlags: null, resource: null };
  }

  _write(severityText, body, extra = {}) {
    if (!shouldLog(severityText)) return;
    const entry = createLogEntry({ severityText, body, ...this._ctx, ...extra });
    const writer = severityText === "ERROR" || severityText === "FATAL" ? this._errorWriter : this._writer;
    writer(entry);
  }

  withContext(ctx) {
    const child = new StructuredLogger({ level: Object.keys(LOG_LEVELS).find((k) => LOG_LEVELS[k] === this._minLevel) });
    child._writer = this._writer;
    child._errorWriter = this._errorWriter;
    child._minLevel = this._minLevel;
    child._ctx = { ...this._ctx, ...ctx };
    return child;
  }

  trace(body, extra) { this._write("TRACE", body, extra); }
  debug(body, extra) { this._write("DEBUG", body, extra); }
  info(body, extra) { this._write("INFO", body, extra); }
  warn(body, extra) { this._write("WARN", body, extra); }

  error(body, extra) {
    const enriched = { ...extra };
    if (body instanceof Error) {
      enriched.exception = body;
      this._write("ERROR", body.message, enriched);
    } else {
      this._write("ERROR", body, enriched);
    }
  }

  fatal(body, extra) {
    const enriched = { ...extra };
    if (body instanceof Error) {
      enriched.exception = body;
      this._write("FATAL", body.message, enriched);
    } else {
      this._write("FATAL", body, enriched);
    }
  }

  log(level, body, extra) {
    const normalized = (typeof level === "string" ? LOG_LEVELS[level.toLowerCase()] : null) || "INFO";
    this._write(normalized, body, extra);
  }

  flush() {}

  static createRequestLogger(req) {
    return req.logger || null;
  }
}

function requestLoggingMiddleware(logger = defaultLogger) {
  return (req, res, next) => {
    const startedAt = process.hrtime.bigint();
    const requestId = crypto.randomUUID();
    const spanId = crypto.randomBytes(8).toString("hex");

    const ctx = {
      traceId: req.traceContext?.traceId || crypto.randomBytes(16).toString("hex"),
      spanId,
      traceFlags: req.traceContext?.traceFlags || "01",
      attributes: {
        "http.request.method": req.method,
        "http.route": req.route?.path || req.path,
        "url.path": req.originalUrl || req.path,
        "url.query": req.url?.includes("?") ? req.url.split("?")[1] : "",
        "network.protocol.version": req.httpVersion,
        "user_agent.original": req.get?.("user-agent") || req.headers?.["user-agent"] || "",
      },
    };

    req.logger = logger.withContext(ctx);
    req.requestId = requestId;

    res.setHeader("x-request-id", requestId);

    res.on("finish", () => {
      const durationNs = Number(process.hrtime.bigint() - startedAt);
      const responseAttrs = {
        "http.response.status_code": res.statusCode,
        "http.response.body.size": res.getHeader("content-length") || 0,
        "network.protocol.version": req.httpVersion,
      };
      const logLevel = res.statusCode >= 500 ? "ERROR" : res.statusCode >= 400 ? "WARN" : "INFO";
      const entry = {
        attributes: { ...ctx.attributes, ...responseAttrs },
        traceId: ctx.traceId,
        spanId: ctx.spanId,
        traceFlags: ctx.traceFlags,
      };
      logger._write(logLevel, `${req.method} ${req.originalUrl || req.path} ${res.statusCode} ${(durationNs / 1e6).toFixed(0)}ms`, entry);
    });

    res.on("close", () => {
      if (!res.writableFinished) {
        logger._write("WARN", "request aborted", {
          attributes: {
            ...ctx.attributes,
            "http.response.status_code": 499,
            "network.protocol.version": req.httpVersion,
          },
          traceId: ctx.traceId,
          spanId: ctx.spanId,
          traceFlags: ctx.traceFlags,
        });
      }
    });

    next();
  };
}

function errorLoggingMiddleware(logger = defaultLogger) {
  return (err, req, res, _next) => {
    const ctx = {
      traceId: req.traceContext?.traceId || req.requestId,
      attributes: {
        "http.request.method": req.method,
        "http.route": req.route?.path || req.path,
        "url.path": req.originalUrl || req.path,
        "http.response.status_code": res.statusCode || 500,
      },
    };

    logger.withContext(ctx).error(err, {
      attributes: {
        "http.request.body": req.body && typeof req.body === "object" ? JSON.stringify(req.body).slice(0, 1024) : undefined,
      },
    });

    if (!res.headersSent) {
      res.status(500).json({ error: "Internal server error", request_id: req.requestId });
    }
  };
}

function createAuditLogEntry({ action, actor, resource, details, traceId, spanId }) {
  return createLogEntry({
    severityText: "INFO",
    body: action,
    traceId,
    spanId,
    attributes: {
      "audit.action": action,
      "audit.actor": actor,
      "audit.resource": resource,
      "audit.details": details ? JSON.stringify(details) : undefined,
      "audit.type": "audit",
    },
  });
}

const defaultLogger = new StructuredLogger();

module.exports = {
  StructuredLogger,
  requestLoggingMiddleware,
  errorLoggingMiddleware,
  createLogEntry,
  createAuditLogEntry,
  setServiceName,
  setAppVersion,
  setEnvironment,
  setLogLevel,
  resetInstanceId,
  defaultLogger,
};
