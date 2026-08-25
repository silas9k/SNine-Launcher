import assert from "node:assert/strict";
import test from "node:test";
import { inspectWorkflowText } from "../../scripts/check-workflow-guards.mjs";

const guarded = `jobs:
  verify:
    steps:
      - run: node scripts/check-public-registry.mjs
      - run: node scripts/check-source-cleanliness.mjs --allow-root-vcs-metadata
      - run: npm ci --registry=https://registry.npmjs.org/
`;

test("accepts both guards before npm ci", () => {
  assert.deepEqual(inspectWorkflowText("good.yml", guarded), []);
});

test("rejects either missing guard and guards placed after npm ci", () => {
  for (const source of [
    guarded.replace("      - run: node scripts/check-public-registry.mjs\n", ""),
    guarded.replace("      - run: node scripts/check-source-cleanliness.mjs --allow-root-vcs-metadata\n", ""),
    `jobs:\n  verify:\n    steps:\n      - run: npm ci\n      - run: node scripts/check-public-registry.mjs\n      - run: node scripts/check-source-cleanliness.mjs\n`,
  ]) {
    assert.ok(inspectWorkflowText("bad.yml", source).length > 0);
  }
});

test("does not reuse guards from an earlier job", () => {
  const source = `${guarded}  second:\n    steps:\n      - run: npm ci\n`;
  assert.equal(inspectWorkflowText("jobs.yml", source).length, 2);
});

test("rejects guards that exist only in YAML comments", () => {
  const source = `jobs:\n  verify:\n    steps:\n      # - run: node scripts/check-public-registry.mjs\n      # - run: node scripts/check-source-cleanliness.mjs --allow-root-vcs-metadata\n      - run: npm ci\n`;
  assert.equal(inspectWorkflowText("comment.yml", source).length, 2);
});

test("rejects guards with continue-on-error true", () => {
  const source = guarded.replace(
    "      - run: node scripts/check-public-registry.mjs\n",
    "      - run: node scripts/check-public-registry.mjs\n        continue-on-error: true\n",
  );
  assert.ok(inspectWorkflowText("continue.yml", source).some((entry) => entry.includes("Registry-Guard")));
});

test("rejects guards that suppress native failure", () => {
  const source = guarded.replace(
    "node scripts/check-public-registry.mjs",
    "node scripts/check-public-registry.mjs || true",
  );
  assert.ok(inspectWorkflowText("suppressed.yml", source).some((entry) => entry.includes("Registry-Guard")));
});

test("accepts exact guards in semantic multiline run steps", () => {
  const source = `jobs:
  verify:
    steps:
      - name: Registry
        run: |
          node scripts/check-public-registry.mjs
      - name: Source
        run: |
          node scripts/check-source-cleanliness.mjs --allow-root-vcs-metadata
      - name: Install
        run: |
          npm ci --registry=https://registry.npmjs.org/
`;
  assert.deepEqual(inspectWorkflowText("multiline.yml", source), []);
});

test("rejects an inline if field on a guard step", () => {
  const source = guarded.replace(
    "      - run: node scripts/check-public-registry.mjs\n",
    "      - if: false\n        run: node scripts/check-public-registry.mjs\n",
  );
  assert.ok(inspectWorkflowText("inline-if.yml", source).some((entry) => entry.includes("Registry-Guard")));
});

test("rejects inline continue-on-error true on a guard step", () => {
  const source = guarded.replace(
    "      - run: node scripts/check-public-registry.mjs\n",
    "      - continue-on-error: true\n        run: node scripts/check-public-registry.mjs\n",
  );
  assert.ok(inspectWorkflowText("inline-continue.yml", source).some((entry) => entry.includes("Registry-Guard")));
});

test("recognizes a folded npm ci command and quoted job name", () => {
  const source = `jobs:
  "quoted verify job":
    steps:
      - run: >
          npm
          ci
`;
  assert.equal(inspectWorkflowText("folded.yml", source).length, 2);
});

test("accepts supported YAML flow mappings with exact separate guards", () => {
  const source = `jobs: { verify: { steps: [
    { run: "node scripts/check-public-registry.mjs" },
    { run: "node scripts/check-source-cleanliness.mjs --allow-root-vcs-metadata" },
    { run: "npm ci" }
  ] } }`;
  assert.deepEqual(inspectWorkflowText("flow.yml", source), []);
});

test("fails closed on YAML parse errors and unevaluable workflow structures", () => {
  assert.ok(inspectWorkflowText("parse.yml", "jobs: { broken: [").length > 0);
  assert.ok(inspectWorkflowText("structure.yml", "jobs: []").length > 0);
  assert.ok(inspectWorkflowText("steps.yml", "jobs:\n  verify:\n    steps: nope\n").length > 0);
});
