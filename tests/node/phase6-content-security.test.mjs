import assert from "node:assert/strict";
import test from "node:test";
import {
  inspectPhase6ContentSecurity,
  phase6CommandErrors,
  phase6ContractSlice,
  phase6PublicRiskFields,
} from "../../scripts/check-phase6-content-security.mjs";

const REQUIRED_COMMANDS = [
  "phase6_content_snapshot",
  "phase6_check_content_updates",
  "phase6_modrinth_search",
  "phase6_modrinth_project",
  "phase6_install_modrinth",
  "phase6_set_content_enabled",
  "phase6_remove_content",
  "phase6_update_content",
  "phase6_add_local_file",
  "phase6_import_modrinth_pack",
  "phase6_export_profile",
  "phase6_import_profile",
];

test("phase6 project passes the complete content-security gate", () => {
  assert.deepEqual(inspectPhase6ContentSecurity(), []);
});

test("phase6 public surfaces reject raw locations and secret fields", () => {
  const forbidden = [
    "raw", "Url ", "download", "Url ", "provider", "Url ",
    "access", "Token ", "refresh", "Token ", "device", "Code",
  ].join("");
  assert.deepEqual(phase6PublicRiskFields(forbidden), [
    "rawUrl",
    "downloadUrl",
    "providerUrl",
    "accessToken",
    "refreshToken",
    "deviceCode",
  ]);
  assert.deepEqual(
    phase6PublicRiskFields("projectId versionId fileName sha256 reasonCode"),
    [],
  );
});

test("phase6 contract slicing cannot hide a dangerous field among older types", () => {
  const contract = {
    version: 6,
    types: {
      LegacyTransport: { rawUrl: "string" },
      Phase6Safe: { projectId: "string" },
    },
    commands: [
      { name: "legacy_transport" },
      { name: "phase6_safe" },
    ],
  };
  assert.deepEqual(phase6PublicRiskFields(phase6ContractSlice(contract)), []);
  contract.types.Phase6Safe.downloadUrl = "string";
  assert.deepEqual(phase6PublicRiskFields(phase6ContractSlice(contract)), ["downloadUrl"]);
});

test("phase6 command gate rejects contract downgrade and missing registration", () => {
  const contract = {
    version: 5,
    commands: REQUIRED_COMMANDS.map((name) => ({ name })),
  };
  const registrations = REQUIRED_COMMANDS.slice(1)
    .map((command) => `ipc::${command}`)
    .join("\n");
  const errors = phase6CommandErrors(contract, registrations);
  assert.ok(errors.includes("IPC-Vertrag ist älter als Version 6."));
  assert.ok(errors.some((error) => error.includes("phase6_content_snapshot")));
});
