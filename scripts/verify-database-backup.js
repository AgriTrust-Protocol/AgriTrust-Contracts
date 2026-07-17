#!/usr/bin/env node
"use strict";

const { createVerifier } = require("../src/services/backupVerification");

async function main() {
  const verifier = createVerifier();
  const result = await verifier.verify();
  console.log(JSON.stringify(result, null, 2));
}

main().catch(error => {
  console.error(JSON.stringify({ status: "failed", error: error.message }, null, 2));
  process.exitCode = 1;
});
