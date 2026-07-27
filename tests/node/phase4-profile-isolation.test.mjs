import assert from "node:assert/strict";
import test from "node:test";
import {
  forbiddenCopyMechanisms,
  inspectPhase4ProfileIsolation,
  permanentQuarantineDeletion,
} from "../../scripts/check-phase4-profile-isolation.mjs";

test("phase4 project passes the complete profile-isolation and cache-safety gate", () => {
  assert.deepEqual(inspectPhase4ProfileIsolation(), []);
});

test("mutable profile trees reject hardlink and unverified clone mechanisms", () => {
  const forbidden = ["fs::hard", "_link(source, target); reflink", "(source, target); clonefile", "(source, target)"].join("");
  assert.deepEqual(forbiddenCopyMechanisms(forbidden), ["hardlink", "reflink", "reflink"]);
});

test("ordinary verified copies remain permitted", () => {
  assert.deepEqual(forbiddenCopyMechanisms("secure_fs::copy_new(&source, &destination)?;"), []);
});

test("cache quarantine rejects permanent deletion before a safety period exists", () => {
  const forbidden = ["fn purge_", "quarantined(path: &Path) { fs::remove_", "file(path); }"].join("");
  assert.equal(permanentQuarantineDeletion(forbidden), true);
});

test("test-only fixture cleanup does not count as a product cache deletion policy", () => {
  assert.equal(permanentQuarantineDeletion("#[cfg(test)]\nmod tests { fs::remove_dir_all(root); }"), false);
});
