import { describe, expect, it } from "vitest";
import { typedIpcError } from "../../src/lib/shellCommands";

describe("typed IPC errors", () => {
  const payload = {
    code: "runtime_java_not_found",
    messageKey: "error.runtime_java_not_found",
    params: {},
  };

  it("preserves structured Tauri errors", () => {
    expect(typedIpcError(payload)).toEqual(payload);
  });

  it("decodes the serialized error forms produced by Tauri WebViews", () => {
    const serialized = JSON.stringify(payload);
    expect(typedIpcError(serialized)).toEqual(payload);
    expect(typedIpcError(new Error(serialized))).toEqual(payload);
  });
});
