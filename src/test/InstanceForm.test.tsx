import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { InstanceForm } from "@/components/InstanceForm";
import type { Instance, MinecraftVersion } from "@/types";

const mocks = vi.hoisted(() => ({
  versions: vi.fn(),
  loaderVersions: vi.fn(),
  createInstance: vi.fn(),
  updateInstance: vi.fn(),
  perform: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    versions: mocks.versions,
    loaderVersions: mocks.loaderVersions,
    createInstance: mocks.createInstance,
    updateInstance: mocks.updateInstance,
  },
  errorMessage: (error: unknown) => typeof error === "string" ? error : error instanceof Error ? error.message : "Une erreur inattendue est survenue",
}));

vi.mock("@/state/LauncherContext", () => ({ useLauncher: () => ({ perform: mocks.perform }) }));

const release: MinecraftVersion = { id: "1.21.4", kind: "release", releaseTime: "2024-12-03T10:00:00+00:00" };
const snapshot: MinecraftVersion = { id: "25w02a", kind: "snapshot", releaseTime: "2025-01-08T10:00:00+00:00" };
const created: Instance = {
  id: "instance-1", name: "Mon aventure", versionId: release.id, loader: "vanilla", color: "#8b5cf6",
  memoryMin: 512, memoryMax: 4096, resolutionWidth: 1280, resolutionHeight: 720,
  fullscreen: false, closeOnLaunch: false, jvmArguments: "", createdAt: "2025-01-08T10:00:00Z", totalPlaySeconds: 0,
};

function renderForm() {
  const onClose = vi.fn();
  const onSaved = vi.fn();
  render(<InstanceForm onClose={onClose} onSaved={onSaved} />);
  return { onSaved };
}

afterEach(cleanup);

describe("InstanceForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.loaderVersions.mockResolvedValue([]);
    mocks.createInstance.mockResolvedValue(created);
    mocks.perform.mockImplementation(async (action: () => Promise<unknown>) => action());
  });

  it("affiche les releases, révèle les snapshots et permet de créer une instance", async () => {
    mocks.versions.mockResolvedValue([release, snapshot]);
    const { onSaved } = renderForm();
    const select = screen.getByRole("combobox", { name: "Minecraft" });

    await waitFor(() => expect(select).toHaveValue(release.id));
    expect(within(select).getAllByRole("option")).toHaveLength(1);
    fireEvent.click(screen.getByRole("checkbox", { name: "Afficher les snapshots" }));
    expect(within(select).getAllByRole("option")).toHaveLength(2);
    expect(screen.getByRole("option", { name: "25w02a · Snapshot" })).toBeInTheDocument();

    fireEvent.change(screen.getByRole("textbox", { name: "Nom de l’instance" }), { target: { value: "Mon aventure" } });
    fireEvent.click(screen.getByRole("button", { name: "Créer l’instance" }));
    await waitFor(() => expect(mocks.createInstance).toHaveBeenCalledWith(expect.objectContaining({ name: "Mon aventure", versionId: release.id })));
    expect(onSaved).toHaveBeenCalledWith(created);
  });

  it("permet de choisir la version de Java de l’instance", async () => {
    mocks.versions.mockResolvedValue([release]);
    renderForm();
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Minecraft" })).toHaveValue(release.id));

    fireEvent.change(screen.getByRole("textbox", { name: "Nom de l’instance" }), { target: { value: "Mon aventure" } });
    const java = screen.getByRole("combobox", { name: "Version de Java" });
    expect(java).toHaveValue("");
    fireEvent.change(java, { target: { value: "25" } });
    expect(java).toHaveValue("25");

    fireEvent.click(screen.getByRole("button", { name: "Créer l’instance" }));
    await waitFor(() => expect(mocks.createInstance).toHaveBeenCalledWith(expect.objectContaining({ javaMajor: 25 })));
  });

  it("conserve le choix Java automatique sans valeur explicite", async () => {
    mocks.versions.mockResolvedValue([release]);
    renderForm();
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Minecraft" })).toHaveValue(release.id));

    fireEvent.change(screen.getByRole("textbox", { name: "Nom de l’instance" }), { target: { value: "Mon aventure" } });
    fireEvent.click(screen.getByRole("button", { name: "Créer l’instance" }));
    await waitFor(() => expect(mocks.createInstance).toHaveBeenCalled());
    expect(mocks.createInstance.mock.calls[0][0]).not.toHaveProperty("javaMajor");
  });

  it("configure une instance Horizon en 26.3 avec un build Nightly", async () => {
    const horizonRelease: MinecraftVersion = { id: "26.3", kind: "release", releaseTime: "2026-10-01T00:00:00+00:00" };
    mocks.versions.mockResolvedValue([horizonRelease]);
    mocks.loaderVersions.mockResolvedValue([{ version: "nightly", stable: false }]);
    renderForm();

    const minecraft = screen.getByRole("combobox", { name: "Minecraft" });
    await waitFor(() => expect(minecraft).toHaveValue("26.3"));
    fireEvent.change(screen.getByRole("combobox", { name: "Moteur" }), { target: { value: "horizon" } });

    await waitFor(() => expect(mocks.loaderVersions).toHaveBeenCalledWith("horizon", "26.3"));
    const build = await screen.findByRole("combobox", { name: "Build Horizon" });
    await waitFor(() => expect(build).toHaveValue("nightly"));
    expect(screen.queryByRole("combobox", { name: "Version de Java" })).not.toBeInTheDocument();
    expect(screen.queryByText(/Mémoire maximale/)).not.toBeInTheDocument();

    fireEvent.change(screen.getByRole("textbox", { name: "Nom de l’instance" }), { target: { value: "Horizon" } });
    fireEvent.click(screen.getByRole("button", { name: "Créer l’instance" }));
    await waitFor(() => expect(mocks.createInstance).toHaveBeenCalledWith(expect.objectContaining({
      loader: "horizon", versionId: "26.3", loaderVersion: "nightly",
    })));
  });

  it("explique un échec de chargement et permet une nouvelle tentative", async () => {
    mocks.versions.mockRejectedValueOnce("Connexion au manifeste impossible").mockResolvedValueOnce([release]);
    renderForm();

    expect(await screen.findByRole("alert")).toHaveTextContent("Connexion au manifeste impossible");
    expect(screen.getByRole("combobox", { name: "Minecraft" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Réessayer" }));

    await waitFor(() => expect(screen.getByRole("combobox", { name: "Minecraft" })).toHaveValue(release.id));
    expect(mocks.versions).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
