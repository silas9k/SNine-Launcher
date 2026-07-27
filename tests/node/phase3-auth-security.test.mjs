import assert from "node:assert/strict";
import test from "node:test";
import {
  inspectPhase3Auth,
  publicSecretIdentifiers,
  secretSchemaColumns,
} from "../../scripts/check-phase3-auth-security.mjs";

test("phase3 auth project passes the complete static security gate", () => {
  assert.deepEqual(inspectPhase3Auth(), []);
});

test("public contracts reject every token and device-secret field", () => {
  const forbidden = ["access", "Token,refresh", "Token,device", "Code,session", "Token"].join("");
  assert.deepEqual(publicSecretIdentifiers(forbidden), [
    "accessToken",
    "refreshToken",
    "deviceCode",
    "sessionToken",
  ]);
});

test("sqlite rejects secret-bearing authentication columns", () => {
  const forbidden = ["access", "_token refresh", "_token device", "_code password secret"].join("");
  assert.deepEqual(secretSchemaColumns(forbidden), [
    "access_token",
    "refresh_token",
    "device_code",
    "password",
    "secret",
  ]);
});

test("opaque non-secret vault references remain permitted", () => {
  assert.deepEqual(secretSchemaColumns("vault_ref ownership_verified_at_unix"), []);
  assert.deepEqual(publicSecretIdentifiers("loginId userCode verificationUri"), []);
});
