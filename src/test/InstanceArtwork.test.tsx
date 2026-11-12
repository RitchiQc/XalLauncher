import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { InstanceArtwork } from "@/components/InstanceArtwork";
import type { Instance } from "@/types";

const mocks = vi.hoisted(() => ({ instanceIconPath: vi.fn() }));

vi.mock("@/lib/api", () => ({ api: { instanceIconPath: mocks.instanceIconPath } }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (path: string) => `asset://${path}` }));

const instance: Instance = {
  id: "instance-1", name: "Test", versionId: "1.21.1", loader: "fabric", color: "#8b5cf6",
  memoryMin: 512, memoryMax: 4096, resolutionWidth: 1280, resolutionHeight: 720,
  fullscreen: false, closeOnLaunch: false, jvmArguments: "", createdAt: "2025-01-01T00:00:00Z", totalPlaySeconds: 0,
};

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
});

describe("InstanceArtwork", () => {
  it("affiche l’icône distante d’un modpack", () => {
    const { container } = render(<InstanceArtwork instance={{ ...instance, iconUrl: "https://cdn.modrinth.com/icon.png" }} />);
    const icon = container.querySelector<HTMLImageElement>(".art-icon");
    expect(icon?.getAttribute("src")).toBe("https://cdn.modrinth.com/icon.png");
    expect(mocks.instanceIconPath).not.toHaveBeenCalled();
  });

  it("résout une couverture locale via le protocole asset", async () => {
    mocks.instanceIconPath.mockResolvedValue("C:\\data\\icon.png");
    const { container } = render(<InstanceArtwork instance={{ ...instance, iconFile: "icon.png" }} />);
    await vi.waitFor(() => {
      expect(container.querySelector<HTMLImageElement>(".art-cover")?.getAttribute("src")).toBe("asset://C:\\data\\icon.png");
    });
    expect(container.querySelector(".art-icon-wrap")).toBeNull();
  });

  it("affiche la couverture par défaut et le logo XalLauncher quand aucune image n’est définie", () => {
    const { container } = render(<InstanceArtwork instance={instance} />);
    expect(container.querySelector(".art-cover-default")).not.toBeNull();
    const icon = container.querySelector<HTMLImageElement>(".art-icon");
    expect(icon?.className).toContain("art-icon-default");
    expect(icon?.getAttribute("src")).toBeTruthy();
  });
});
