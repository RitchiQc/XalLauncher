import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectBrowser } from "@/components/ProjectBrowser";
import type { Instance, ModrinthVersion, ProjectHit } from "@/types";

const mocks = vi.hoisted(() => ({
  search: vi.fn(),
  projectVersions: vi.fn(),
  installProject: vi.fn(),
  installModpack: vi.fn(),
  perform: vi.fn(),
  notify: vi.fn(),
  onContentInstalled: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    search: mocks.search,
    projectVersions: mocks.projectVersions,
    installProject: mocks.installProject,
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

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  mocks.search.mockResolvedValue([project]);
  mocks.projectVersions.mockResolvedValue(versions);
  mocks.installProject.mockResolvedValue({ fileName: "sodium.jar" });
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
});
