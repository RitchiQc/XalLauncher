import { afterEach, describe, expect, it, vi } from "vitest";
import { confirmAction } from "@/lib/dialog";

const mocks = vi.hoisted(() => ({ ask: vi.fn() }));

vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: mocks.ask }));

describe("confirmAction", () => {
  afterEach(() => { vi.clearAllMocks(); vi.unstubAllGlobals(); });

  it("renvoie la réponse de la boîte de dialogue native", async () => {
    mocks.ask.mockResolvedValue(true);
    await expect(confirmAction("Continuer ?")).resolves.toBe(true);
    expect(mocks.ask).toHaveBeenCalledWith("Continuer ?", expect.objectContaining({ okLabel: "Oui", cancelLabel: "Non" }));
  });

  it("ne confirme jamais une action quand le dialogue échoue", async () => {
    mocks.ask.mockRejectedValue(new Error("dialogue indisponible"));
    vi.stubGlobal("confirm", () => undefined);
    await expect(confirmAction("Continuer ?")).resolves.toBe(false);
  });
});
