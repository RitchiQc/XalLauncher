import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AccountsPage } from "@/pages/AccountsPage";

const mocks = vi.hoisted(() => ({
  login: vi.fn(),
  cancelLogin: vi.fn(),
  openPrivate: vi.fn(),
  saveSettings: vi.fn(),
  perform: vi.fn(),
  refreshAccounts: vi.fn(),
  setActiveAccountId: vi.fn(),
  setSettings: vi.fn(),
  clearDeviceCode: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    login: mocks.login,
    cancelLogin: mocks.cancelLogin,
    openPrivate: mocks.openPrivate,
    saveSettings: mocks.saveSettings,
  },
  errorMessage: (error: unknown) => typeof error === "string" ? error : "Erreur",
}));
vi.mock("@/state/LauncherContext", () => ({
  useLauncher: () => ({
    accounts: [],
    activeAccount: undefined,
    activeAccountId: "",
    setActiveAccountId: mocks.setActiveAccountId,
    settings: {
      language: "fr", theme: "dark", accent: "#8b5cf6", reducedMotion: false,
      downloadConcurrency: 8, microsoftClientId: "client-id", discordEnabled: false, autoUpdate: true,
    },
    setSettings: mocks.setSettings,
    refreshAccounts: mocks.refreshAccounts,
    perform: mocks.perform,
    deviceCode: undefined,
    clearDeviceCode: mocks.clearDeviceCode,
  }),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@/lib/dialog", () => ({ confirmAction: vi.fn() }));
vi.mock("@/components/SkinPreview", () => ({ SkinPreview: () => null }));

beforeEach(() => {
  vi.clearAllMocks();
  mocks.perform.mockImplementation(async (action: () => Promise<unknown>) => action());
  mocks.login.mockResolvedValue({ id: "uuid", name: "Player", capes: [], expiresAt: "2026-01-01T00:00:00Z" });
  mocks.refreshAccounts.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe("AccountsPage", () => {
  it("conserve le code d’appareil comme mode par défaut", async () => {
    render(<AccountsPage />);
    fireEvent.click(screen.getByRole("button", { name: /connexion microsoft/i }));
    fireEvent.click(screen.getByRole("button", { name: /continuer avec microsoft/i }));
    await waitFor(() => expect(mocks.login).toHaveBeenCalledWith("client-id", "device"));
  });

  it("ouvre directement la page Microsoft en mode autre compte", async () => {
    render(<AccountsPage />);
    fireEvent.click(screen.getByRole("button", { name: /connexion microsoft/i }));
    fireEvent.click(screen.getByRole("button", { name: /utiliser un autre compte microsoft/i }));
    expect(screen.getByRole("button", { name: /ouvrir la page microsoft/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /ouvrir la page microsoft/i }));
    await waitFor(() => expect(mocks.login).toHaveBeenCalledWith("client-id", "browser"));
  });
});
