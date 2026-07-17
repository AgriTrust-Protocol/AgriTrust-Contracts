"use strict";

const fs = require("fs");
const path = require("path");

const manifestPath = path.join(__dirname, "..", "k8s", "istio", "agritrust-service-mesh.yaml");
const manifest = fs.readFileSync(manifestPath, "utf8");

describe("service mesh mTLS manifest", () => {
  it("enforces strict mutual TLS for the namespace", () => {
    expect(manifest).toContain("kind: PeerAuthentication");
    expect(manifest).toContain("mode: STRICT");
  });

  it("applies default-deny before explicit allow policies", () => {
    expect(manifest).toContain("name: agritrust-default-deny");
    expect(manifest).toContain("kind: AuthorizationPolicy");
    expect(manifest).toContain("action: ALLOW");
  });

  it("uses Istio mutual TLS and canary traffic splitting", () => {
    expect(manifest).toContain("mode: ISTIO_MUTUAL");
    expect(manifest).toContain("subset: blue");
    expect(manifest).toContain("subset: green");
    expect(manifest).toContain("timeout: 100ms");
  });
});
