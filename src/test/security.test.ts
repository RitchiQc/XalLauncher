import { afterEach, describe, expect, it, vi } from "vitest";
import { confirmCodeExecution } from "@/lib/security";

const mocks = vi.hoisted(() => ({ ask: vi.fn() }));

vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: mocks.ask }));

describe("confirmCodeExecution", () => {
  afterEach(() => vi.clearAllMocks());

  it("explique le risque avant d’autoriser l’installation", async () => {
    mocks.ask.mockResolvedValue(true);
    await expect(confirmCodeExecution("Ce mod")).resolves.toBe(true);
    expect(mocks.ask).toHaveBeenCalledOnce();
    expect(mocks.ask.mock.calls[0][0]).toContain("jeton de session Minecraft");
  });

  it("respecte le refus de l’utilisateur", async () => {
    mocks.ask.mockResolvedValue(false);
    await expect(confirmCodeExecution("Ce modpack")).resolves.toBe(false);
  });
});

