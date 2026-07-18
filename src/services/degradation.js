"use strict";

const DEFAULTS = Object.freeze({
  shedCapacity: false,
  mutationEndpoints: true,
  escrowRead: true,
});

const state = {
  flags: { ...DEFAULTS },
  maxInFlight: Number.POSITIVE_INFINITY,
  inFlight: 0,
  counters: {
    accepted: 0,
    completed: 0,
    shed: 0,
    disabled: 0,
  },
};

function parseBoolean(value, fallback) {
  if (value === undefined || value === null || value === "") return fallback;
  if (typeof value === "boolean") return value;
  const normalised = String(value).trim().toLowerCase();
  if (["1", "true", "yes", "on", "enabled"].includes(normalised)) return true;
  if (["0", "false", "no", "off", "disabled"].includes(normalised)) return false;
  return fallback;
}

function parsePositiveInteger(value, fallback) {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function loadFlagsFromEnv(env = process.env) {
  let parsed = {};
  if (env.FEATURE_FLAGS) {
    try {
      parsed = JSON.parse(env.FEATURE_FLAGS);
    } catch (_err) {
      parsed = {};
    }
  }

  return {
    shedCapacity: parseBoolean(env.FEATURE_SHED_CAPACITY ?? parsed.shedCapacity, DEFAULTS.shedCapacity),
    mutationEndpoints: parseBoolean(
      env.FEATURE_ESCROW_MUTATIONS ?? parsed.mutationEndpoints,
      DEFAULTS.mutationEndpoints
    ),
    escrowRead: parseBoolean(env.FEATURE_ESCROW_READ ?? parsed.escrowRead, DEFAULTS.escrowRead),
  };
}

function configureDegradation(options = {}) {
  const env = options.env || process.env;
  state.flags = options.flags ? { ...DEFAULTS, ...options.flags } : loadFlagsFromEnv(env);
  state.maxInFlight = parsePositiveInteger(
    options.maxInFlight ?? env.CAPACITY_SHED_MAX_IN_FLIGHT,
    Number.POSITIVE_INFINITY
  );
}

function resetDegradation(overrides = {}) {
  state.flags = { ...DEFAULTS, ...(overrides.flags || {}) };
  state.maxInFlight = overrides.maxInFlight ?? Number.POSITIVE_INFINITY;
  state.inFlight = 0;
  state.counters = { accepted: 0, completed: 0, shed: 0, disabled: 0 };
}

function getDegradationSnapshot() {
  return {
    flags: { ...state.flags },
    capacity: {
      in_flight: state.inFlight,
      max_in_flight: Number.isFinite(state.maxInFlight) ? state.maxInFlight : null,
      shedding_enabled: state.flags.shedCapacity,
    },
    counters: { ...state.counters },
  };
}

function featureFlagGate(featureName) {
  return (_req, res, next) => {
    if (state.flags[featureName] === false) {
      state.counters.disabled += 1;
      return res.status(503).json({
        error: "Feature temporarily disabled",
        feature: featureName,
      });
    }
    return next();
  };
}

function capacityShedding(req, res, next) {
  if (req.path === "/ops/degradation" || req.path === "/healthz") return next();

  if (state.flags.shedCapacity && state.inFlight >= state.maxInFlight) {
    state.counters.shed += 1;
    res.set("Retry-After", "1");
    return res.status(503).json({ error: "Capacity temporarily unavailable" });
  }

  state.inFlight += 1;
  state.counters.accepted += 1;

  res.on("finish", () => {
    state.inFlight = Math.max(0, state.inFlight - 1);
    state.counters.completed += 1;
  });

  return next();
}

configureDegradation();

module.exports = {
  capacityShedding,
  configureDegradation,
  featureFlagGate,
  getDegradationSnapshot,
  resetDegradation,
};
