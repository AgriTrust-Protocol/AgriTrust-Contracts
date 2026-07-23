"use strict";

const {
  StructuredLogger,
  createLogEntry,
  setServiceName,
  setAppVersion,
  setEnvironment,
  setLogLevel,
  defaultLogger,
} = require("../src/services/structuredLogger");

describe("createLogEntry", () => {
  it("produces a JSON-serialisable entry with required fields", () => {
    const entry = createLogEntry({
      severityText: "INFO",
      body: "test message",
      traceId: "abc123",
    });

    expect(entry).toHaveProperty("timestamp");
    expect(entry.severity_text).toBe("INFO");
    expect(entry.severity_number).toBe(9);
    expect(entry.body).toBe("test message");
    expect(entry.trace_id).toBe("abc123");
    expect(entry.service_name).toBeTruthy();
  });

  it("includes exception fields when provided", () => {
    const err = new Error("boom");
    const entry = createLogEntry({
      severityText: "ERROR",
      body: "something failed",
      exception: err,
    });

    expect(entry.attributes["exception.type"]).toBe("Error");
    expect(entry.attributes["exception.message"]).toBe("boom");
    expect(entry.attributes["exception.stacktrace"]).toBeTruthy();
  });

  it("serialises nested attribute values", () => {
    const entry = createLogEntry({
      severityText: "INFO",
      attributes: { nested: { foo: "bar" }, simple: 42 },
    });

    expect(typeof entry.attributes.nested).toBe("string");
    expect(entry.attributes.simple).toBe(42);
  });

  it("truncates long bodies", () => {
    const long = "x".repeat(5000);
    const entry = createLogEntry({ severityText: "INFO", body: long });
    expect(entry.body.length).toBeLessThan(5000);
  });
});

describe("StructuredLogger", () => {
  let lines;

  beforeEach(() => {
    lines = [];
    setLogLevel("trace");
  });

  function captureWriter(msg) {
    lines.push(JSON.parse(msg));
  }

  it("logs at INFO level by default", () => {
    const logger = new StructuredLogger({ writer: captureWriter });
    logger.info("hello");
    expect(lines).toHaveLength(1);
    expect(lines[0].severity_text).toBe("INFO");
    expect(lines[0].body).toBe("hello");
  });

  it("respects minimum log level", () => {
    setLogLevel("warn");
    const logger = new StructuredLogger({ writer: captureWriter, level: "warn" });
    logger.info("should not appear");
    logger.warn("should appear");
    expect(lines).toHaveLength(1);
    expect(lines[0].body).toBe("should appear");
  });

  it("logs Error objects with exception fields", () => {
    const logger = new StructuredLogger({ writer: captureWriter });
    logger.error(new Error("fail"));
    expect(lines[0].severity_text).toBe("ERROR");
    expect(lines[0].attributes["exception.type"]).toBe("Error");
    expect(lines[0].attributes["exception.message"]).toBe("fail");
  });

  it("supports child loggers with context", () => {
    const logger = new StructuredLogger({ writer: captureWriter });
    const child = logger.withContext({ traceId: "trace-1" });
    child.info("child log");
    expect(lines[0].trace_id).toBe("trace-1");
  });

  it("can log with log() convenience method", () => {
    const logger = new StructuredLogger({ writer: captureWriter });
    logger.log("warn", "convenient");
    expect(lines[0].severity_text).toBe("WARN");
  });

  it("outputs to stderr for ERROR/FATAL", () => {
    const stdoutLines = [];
    const stderrLines = [];
    const logger = new StructuredLogger({
      writer: (e) => stdoutLines.push(e),
      errorWriter: (e) => stderrLines.push(e),
    });
    logger.info("stdout");
    logger.error("stderr");
    expect(stdoutLines).toHaveLength(1);
    expect(stderrLines).toHaveLength(1);
  });

  it("handles fatal severity", () => {
    const logger = new StructuredLogger({ writer: captureWriter });
    logger.fatal("critical");
    expect(lines[0].severity_text).toBe("FATAL");
    expect(lines[0].severity_number).toBe(21);
  });

  it("debug and trace are silent above min level", () => {
    setLogLevel("info");
    const logger = new StructuredLogger({ writer: captureWriter, level: "info" });
    logger.trace("trace");
    logger.debug("debug");
    logger.info("info");
    expect(lines).toHaveLength(1);
    expect(lines[0].body).toBe("info");
  });
});

describe("requestLoggingMiddleware integration", () => {
  it("adds logger and requestId to req", () => {
    const { requestLoggingMiddleware } = require("../src/services/structuredLogger");
    const logger = new StructuredLogger({ writer: () => {} });
    const middleware = requestLoggingMiddleware(logger);

    const req = { method: "GET", path: "/test", url: "/test", get: () => undefined, headers: {} };
    const res = { setHeader: () => {}, on: () => {}, writableFinished: true };
    const next = jest.fn();

    middleware(req, res, next);
    expect(req.logger).toBeInstanceOf(StructuredLogger);
    expect(req.requestId).toBeTruthy();
    expect(next).toHaveBeenCalled();
  });

  it("logs 499 on connection close without finish", () => {
    const lines = [];
    const logger = new StructuredLogger({ writer: (e) => lines.push(JSON.parse(e)) });
    const { requestLoggingMiddleware } = require("../src/services/structuredLogger");
    const middleware = requestLoggingMiddleware(logger);

    const req = { method: "GET", path: "/test", url: "/test", get: () => undefined, headers: {} };
    const closeHandlers = [];
    const finishHandlers = [];
    const res = {
      setHeader: () => {},
      on: (ev, h) => { if (ev === "close") closeHandlers.push(h); if (ev === "finish") finishHandlers.push(h); },
      writableFinished: false,
    };
    const next = jest.fn();

    middleware(req, res, next);
    closeHandlers.forEach((h) => h());
    const abortedLogs = lines.filter((l) => l.attributes?.["http.response.status_code"] === 499);
    expect(abortedLogs).toHaveLength(1);
  });
});

describe("errorLoggingMiddleware", () => {
  it("logs error and sends 500 response", () => {
    const lines = [];
    const logger = new StructuredLogger({ writer: (e) => lines.push(JSON.parse(e)) });
    const { errorLoggingMiddleware } = require("../src/services/structuredLogger");
    const middleware = errorLoggingMiddleware(logger);

    const err = new Error("test error");
    const req = { method: "GET", path: "/fail", url: "/fail", traceContext: { traceId: "t1" } };
    const res = { status: () => res, json: () => {}, headersSent: false };
    const next = jest.fn();

    middleware(err, req, res, next);
    expect(lines).toHaveLength(1);
    expect(lines[0].severity_text).toBe("ERROR");
    expect(lines[0].attributes["exception.message"]).toBe("test error");
  });
});

describe("createAuditLogEntry", () => {
  it("creates an audit entry with standard fields", () => {
    const { createAuditLogEntry } = require("../src/services/structuredLogger");
    const entry = createAuditLogEntry({
      action: "grant.created",
      actor: "admin@agritrust",
      resource: "grant/42",
      details: { amount: 1000 },
      traceId: "trace-abc",
    });

    expect(entry.severity_text).toBe("INFO");
    expect(entry.attributes["audit.action"]).toBe("grant.created");
    expect(entry.attributes["audit.actor"]).toBe("admin@agritrust");
    expect(entry.attributes["audit.type"]).toBe("audit");
    expect(entry.trace_id).toBe("trace-abc");
  });
});
