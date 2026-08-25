import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { inspectSourceTree } from "../../scripts/check-source-cleanliness.mjs";

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "s9-source-clean-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

test("accepts an isolated clean source tree", (t) => {
  const root = fixture(t);
  fs.writeFileSync(path.join(root, "source.txt"), "clean\n");
  assert.deepEqual(inspectSourceTree(root).errors, []);
});

test("rejects blocked directory names case-insensitively", (t) => {
  const root = fixture(t);
  for (const name of ["NODE_MODULES", "Target", "Dist", ".Git"]) fs.mkdirSync(path.join(root, name));
  assert.equal(inspectSourceTree(root).errors.length, 4);
});

test("rejects .git files and nested metadata but permits explicit root CI metadata", (t) => {
  const root = fixture(t);
  fs.writeFileSync(path.join(root, ".Git"), "gitdir: elsewhere\n");
  assert.equal(inspectSourceTree(root).errors.length, 1);
  assert.deepEqual(inspectSourceTree(root, { allowRootVcsMetadata: true }).errors, []);
  fs.unlinkSync(path.join(root, ".Git"));
  fs.mkdirSync(path.join(root, "nested"));
  fs.writeFileSync(path.join(root, "nested", ".git"), "metadata\n");
  assert.equal(inspectSourceTree(root, { allowRootVcsMetadata: true }).errors.length, 1);
});

test("rejects common backup, patch and temporary remnants", (t) => {
  const root = fixture(t);
  for (const name of ["code.orig", "change.PATCH", "draft.tmp", "file~", "#autosave#", ".~lock.note"]) {
    fs.writeFileSync(path.join(root, name), "fixture\n");
  }
  assert.ok(inspectSourceTree(root).errors.length >= 6);
});

test("rejects symbolic links without following them", (t) => {
  const root = fixture(t);
  const target = path.join(root, "target-dir");
  fs.mkdirSync(target);
  const link = path.join(root, "linked-dir");
  fs.symlinkSync(target, link, process.platform === "win32" ? "junction" : "dir");
  assert.ok(inspectSourceTree(root).errors.some((entry) => entry.includes("symbolische Verknüpfungen")));
});
