import { afterEach, describe, expect, it, vi } from "vitest";
import { confirmCodeExecution } from "@/lib/security";

describe("confirmCodeExecution", () => {
  afterEach(() => vi.restoreAllMocks());

  it("explique le risque avant d’autoriser l’installation", () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    expect(confirmCodeExecution("Ce mod")).toBe(true);
    expect(confirm).toHaveBeenCalledOnce();
    expect(confirm.mock.calls[0][0]).toContain("jeton de session Minecraft");
  });

  it("respecte le refus de l’utilisateur", () => {
    vi.spyOn(window, "confirm").mockReturnValue(false);
    expect(confirmCodeExecution("Ce modpack")).toBe(false);
  });
});
