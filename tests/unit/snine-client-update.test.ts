import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  SNINE_CLIENT_DOWNLOAD_UPDATE,
  SNINE_CLIENT_UPDATE_CHECK,
} from "../../src/lib/generated/ipc-contracts";

const invoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { snineClientUpdate } from "../../src/lib/snineClientUpdate";

beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("SNine Client update commands", () => {
  it("uses the registered typed check command", async () => {
    const response = {
      reachable: true,
      updateAvailable: false,
      externalClientInstalled: true,
      installedVersion: "1.1.5",
      remoteVersion: "1.1.5",
      remoteSizeBytes: 4096,
      statusMessage: "up_to_date",
    };
    invoke.mockResolvedValue(response);

    await expect(snineClientUpdate.check("profile-one")).resolves.toEqual(response);
    expect(invoke).toHaveBeenCalledWith(SNINE_CLIENT_UPDATE_CHECK, {
      profileId: "profile-one",
    });
  });

  it("uses the registered typed download command", async () => {
    const response = {
      installedVersion: "1.1.5",
      sha256: "a".repeat(64),
      sizeBytes: 4096,
      targetFile: "snineclient.jar",
    };
    invoke.mockResolvedValue(response);

    await expect(snineClientUpdate.download("profile-one")).resolves.toEqual(response);
    expect(invoke).toHaveBeenCalledWith(SNINE_CLIENT_DOWNLOAD_UPDATE, {
      profileId: "profile-one",
    });
  });
});
