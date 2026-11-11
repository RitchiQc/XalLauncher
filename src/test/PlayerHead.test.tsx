import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CapeThumbnail } from "@/components/CapeThumbnail";
import { PlayerHead } from "@/components/PlayerHead";

let textureSize = { width: 64, height: 64 };
let textureFails = false;

class MockImage {
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  naturalWidth = 0;
  naturalHeight = 0;
  set src(_value: string) {
    queueMicrotask(() => {
      this.naturalWidth = textureSize.width;
      this.naturalHeight = textureSize.height;
      if (textureFails) this.onerror?.();
      else this.onload?.();
    });
  }
}

beforeEach(() => {
  textureSize = { width: 64, height: 64 };
  textureFails = false;
  vi.stubGlobal("Image", MockImage);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("PlayerHead", () => {
  it("dessine la tête depuis la texture officielle mesurée, sans service tiers", async () => {
    textureSize = { width: 64, height: 32 };
    render(<PlayerHead skinUrl="https://textures.minecraft.net/texture/abc" uuid="uuid" name="RitchiQc" />);
    const head = await screen.findByRole("img", { name: "Tête de RitchiQc" });
    expect(head.style.backgroundImage).toContain("textures.minecraft.net/texture/abc");
    expect(head.style.backgroundSize).toBe("344px 172px");
    expect(head.style.backgroundPosition).toBe("-43px -43px");
    const overlay = head.querySelector<HTMLElement>(".player-head-overlay");
    expect(overlay?.style.backgroundPosition).toBe("-215px -43px");
  });

  it("adapte le cadrage aux skins haute définition", async () => {
    textureSize = { width: 128, height: 128 };
    render(<PlayerHead skinUrl="https://textures.minecraft.net/texture/hd" uuid="uuid" name="RitchiQc" />);
    const head = await screen.findByRole("img", { name: "Tête de RitchiQc" });
    expect(head.style.backgroundSize).toBe("688px 688px");
  });

  it("utilise un avatar distant quand aucun skin n’est disponible", () => {
    render(<PlayerHead uuid="0123456789abcdef" name="Invité" />);
    const head = screen.getByRole("img", { name: "Tête de Invité" });
    expect(head.getAttribute("src")).toContain("mc-heads.net/avatar/0123456789abcdef");
  });

  it("bascule sur un avatar distant quand la texture est illisible", async () => {
    textureFails = true;
    render(<PlayerHead skinUrl="https://textures.minecraft.net/texture/broken" uuid="0123456789abcdef" name="Invité" />);
    const head = await screen.findByRole("img", { name: "Tête de Invité" });
    expect(head.getAttribute("src")).toContain("mc-heads.net/avatar/0123456789abcdef");
  });

  it("n’affiche jamais de carré noir quand toutes les sources échouent", () => {
    render(<PlayerHead uuid="0123456789abcdef" name="RitchiQc" />);
    fireEvent.error(screen.getByRole("img", { name: "Tête de RitchiQc" }));
    fireEvent.error(screen.getByRole("img", { name: "Tête de RitchiQc" }));
    expect(screen.getByRole("img", { name: "Tête de RitchiQc" }).textContent).toBe("R");
  });
});

describe("CapeThumbnail", () => {
  it("découpe la face extérieure de la cape", async () => {
    textureSize = { width: 64, height: 32 };
    render(<CapeThumbnail url="https://textures.minecraft.net/texture/cape" alias="Migrator" />);
    const cape = await screen.findByRole("img", { name: "Cape Migrator" });
    expect(cape.style.backgroundImage).toContain("textures.minecraft.net/texture/cape");
    expect(cape.style.backgroundSize).toBe("256px 128px");
    expect(cape.style.backgroundPosition).toBe("-4px -4px");
  });

  it("gère les capes legacy 22x17", async () => {
    textureSize = { width: 22, height: 17 };
    render(<CapeThumbnail url="https://textures.minecraft.net/texture/old" alias="MineCon" />);
    const cape = await screen.findByRole("img", { name: "Cape MineCon" });
    expect(cape.style.backgroundSize).toBe("88px 68px");
    expect(cape.style.backgroundPosition).toBe("-4px -4px");
  });

  it("gère les capes haute définition", async () => {
    textureSize = { width: 128, height: 64 };
    render(<CapeThumbnail url="https://textures.minecraft.net/texture/cape-hd" alias="HD" />);
    const cape = await screen.findByRole("img", { name: "Cape HD" });
    expect(cape.style.backgroundSize).toBe("1024px 512px");
    expect(cape.style.backgroundPosition).toBe("-8px -8px");
  });
});
