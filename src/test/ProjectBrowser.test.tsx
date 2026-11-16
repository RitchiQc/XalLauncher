import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectBrowser } from "@/components/ProjectBrowser";
import type { ContentEntry, Instance, ModrinthVersion, ProjectHit } from "@/types";

const mocks = vi.hoisted(() => ({
  search: vi.fn(),
  searchHorizon: vi.fn(),
  projectVersions: vi.fn(),
  horizonVersions: vi.fn(),
  installProject: vi.fn(),
  installHorizonProject: vi.fn(),
  installModpack: vi.fn(),
  perform: vi.fn(),
  notify: vi.fn(),
  onContentInstalled: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    search: mocks.search,
    searchHorizon: mocks.searchHorizon,
    projectVersions: mocks.projectVersions,
    horizonVersions: mocks.horizonVersions,
    installProject: mocks.installProject,
    installHorizonProject: mocks.installHorizonProject,
    installModpack: mocks.installModpack,
  },
  errorMessage: (error: unknown) => typeof error === "string" ? error : "Erreur",
}));
vi.mock("@/state/LauncherContext", () => ({
  useLauncher: () => ({ perform: mocks.perform, notify: mocks.notify }),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

const instance: Instance = {
  id: "instance-1", name: "Test", versionId: "1.21.1", loader: "fabric", color: "#8b5cf6",
  memoryMin: 512, memoryMax: 4096, resolutionWidth: 1280, resolutionHeight: 720,
  fullscreen: false, closeOnLaunch: false, jvmArguments: "", createdAt: "2025-01-01T00:00:00Z", totalPlaySeconds: 0,
};
const project: ProjectHit = {
  projectId: "sodium", slug: "sodium", title: "Sodium", description: "Optimisations", author: "CaffeineMC",
  downloads: 1000, follows: 100, projectType: "mod", versions: ["1.21.1"],
};
const versions: ModrinthVersion[] = [
  { id: "new", name: "Sodium 0.7", versionNumber: "0.7.0", versionType: "release", datePublished: "2025-02-01T00:00:00Z", gameVersions: ["1.21.1"], loaders: ["fabric"] },
  { id: "old", name: "Sodium 0.6", versionNumber: "0.6.13", versionType: "release", datePublished: "2025-01-01T00:00:00Z", gameVersions: ["1.21.1"], loaders: ["fabric"] },
];
const installed: ContentEntry = {
  fileName: "sodium.jar", kind: "mod", enabled: true, size: 100, modifiedAt: "2025-02-01T00:00:00Z",
  projectId: "sodium", projectTitle: "Sodium", versionId: "new",
};
const horizonInstance: Instance = { ...instance, versionId: "26.3", loader: "horizon" };
const horizonProject: ProjectHit = {
  projectId: "coords_hud", slug: "coords_hud", title: "Coordinates HUD", description: "Coordonnées",
  author: "Horizon", downloads: 2000, follows: 50, projectType: "mod", versions: ["26.3"],
  provider: "horizon",
};
const horizonVersions: ModrinthVersion[] = [
  { id: "0.1.0", name: "Coordinates HUD 0.1.0", versionNumber: "0.1.0", versionType: "release", datePublished: "2026-11-09", gameVersions: ["26.3"], loaders: ["horizon"] },
];

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  vi.stubGlobal("IntersectionObserver", class {
    private callback: IntersectionObserverCallback;
    constructor(callback: IntersectionObserverCallback) { this.callback = callback; }
    observe() { this.callback([{ isIntersecting: true } as IntersectionObserverEntry], this as unknown as IntersectionObserver); }
    unobserve() { return undefined; }
    disconnect() { return undefined; }
    takeRecords() { return []; }
  });
  mocks.search.mockResolvedValue([project]);
  mocks.searchHorizon.mockResolvedValue([horizonProject]);
  mocks.projectVersions.mockResolvedValue(versions);
  mocks.horizonVersions.mockResolvedValue(horizonVersions);
  mocks.installProject.mockResolvedValue({ fileName: "sodium.jar" });
  mocks.installHorizonProject.mockResolvedValue({ fileName: "coords_hud.hmod" });
  mocks.perform.mockImplementation(async (action: () => Promise<unknown>) => action());
  mocks.onContentInstalled.mockResolvedValue(undefined);
});

describe("ProjectBrowser", () => {
  it("installe une ancienne version choisie sans avertissement pour un mod Modrinth", async () => {
    const confirm = vi.spyOn(window, "confirm");
    render(<ProjectBrowser kind="mod" instance={instance} onContentInstalled={mocks.onContentInstalled} />);

    fireEvent.click(await screen.findByRole("button", { name: "Installer" }));
    const select = await screen.findByRole("combobox", { name: "Version du contenu" });
    expect(select).toHaveValue("new");
    expect(screen.getByRole("option", { name: "0.6.13 · release" })).toBeInTheDocument();

    fireEvent.change(select, { target: { value: "old" } });
    fireEvent.click(screen.getByRole("button", { name: "Installer cette version" }));

    await waitFor(() => expect(mocks.installProject).toHaveBeenCalledWith("sodium", "instance-1", "old"));
    expect(mocks.projectVersions).toHaveBeenCalledWith("sodium", "instance-1");
    expect(mocks.onContentInstalled).toHaveBeenCalledOnce();
    expect(confirm).not.toHaveBeenCalled();
  });

  it("marque la version installée et permet seulement son remplacement par une autre", async () => {
    render(<ProjectBrowser kind="mod" instance={instance} installedContent={[installed]} onContentInstalled={mocks.onContentInstalled} />);

    expect(await screen.findByText("Installé")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Versions" }));
    const select = await screen.findByRole("combobox", { name: "Version du contenu" });
    expect(select).toHaveValue("new");
    expect(screen.getByRole("option", { name: "0.7.0 · release · installée" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Version installée" })).toBeDisabled();

    fireEvent.change(select, { target: { value: "old" } });
    fireEvent.click(screen.getByRole("button", { name: "Remplacer par cette version" }));
    await waitFor(() => expect(mocks.installProject).toHaveBeenCalledWith("sodium", "instance-1", "old"));
    expect(mocks.onContentInstalled).toHaveBeenCalledOnce();
  });

  it("charge la page suivante avec le bon décalage", async () => {
    const firstPage = Array.from({ length: 24 }, (_, index) => ({
      ...project, projectId: `mod-${index}`, slug: `mod-${index}`, title: `Mod ${index}`,
    }));
    mocks.search.mockResolvedValueOnce(firstPage).mockResolvedValueOnce([project]);

    render(<ProjectBrowser kind="mod" instance={instance} onContentInstalled={mocks.onContentInstalled} />);
    expect(await screen.findByText("Mod 0")).toBeInTheDocument();
    await waitFor(() => expect(mocks.search).toHaveBeenLastCalledWith("", "mod", "1.21.1", "fabric", 24, "relevance"));
    expect(await screen.findByText("Sodium")).toBeInTheDocument();
  });

  it("trie les résultats côté Modrinth", async () => {
    render(<ProjectBrowser kind="mod" instance={instance} onContentInstalled={mocks.onContentInstalled} />);
    await screen.findByText("Sodium");
    fireEvent.change(screen.getByRole("combobox", { name: "Trier les résultats" }), { target: { value: "downloads" } });
    await waitFor(() => expect(mocks.search).toHaveBeenLastCalledWith("", "mod", "1.21.1", "fabric", 0, "downloads"));
  });

  it("interroge le registre Horizon pour une instance Horizon", async () => {
    render(<ProjectBrowser kind="mod" instance={horizonInstance} onContentInstalled={mocks.onContentInstalled} />);

    expect(await screen.findByText("Coordinates HUD")).toBeInTheDocument();
    expect(mocks.searchHorizon).toHaveBeenCalledWith("", "mod", "26.3", "horizon", 0, "relevance");
    expect(mocks.search).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Installer" }));
    const select = await screen.findByRole("combobox", { name: "Version du contenu" });
    expect(select).toHaveValue("0.1.0");
    fireEvent.click(screen.getByRole("button", { name: "Installer cette version" }));

    await waitFor(() => expect(mocks.installHorizonProject).toHaveBeenCalledWith("coords_hud", "instance-1", "0.1.0"));
    expect(mocks.horizonVersions).toHaveBeenCalledWith("coords_hud", "instance-1");
    expect(mocks.onContentInstalled).toHaveBeenCalledOnce();
  });
});
