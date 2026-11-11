import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CapeThumbnail } from "@/components/CapeThumbnail";
import { PlayerHead } from "@/components/PlayerHead";

afterEach(cleanup);

describe("PlayerHead", () => {
  it("dessine la tête depuis la texture officielle plutôt que depuis un service tiers", () => {
    render(<PlayerHead skinUrl="https://textures.minecraft.net/texture/abc" uuid="uuid" name="RitchiQc" />);
    const head = screen.getByRole("img", { name: "Tête de RitchiQc" });
    expect(head.style.backgroundImage).toContain("textures.minecraft.net/texture/abc");
    expect(head.style.backgroundSize).toBe("344px 344px");
  });

  it("utilise un avatar distant quand aucun skin n’est disponible", () => {
    render(<PlayerHead uuid="0123456789abcdef" name="Invité" />);
    const head = screen.getByRole("img", { name: "Tête de Invité" });
    expect(head.getAttribute("src")).toContain("crafatar.com/avatars/0123456789abcdef");
  });
});

describe("CapeThumbnail", () => {
  it("découpe uniquement le panneau arrière de la cape", () => {
    render(<CapeThumbnail url="https://textures.minecraft.net/texture/cape" alias="Migrator" />);
    const cape = screen.getByRole("img", { name: "Cape Migrator" });
    expect(cape.style.backgroundImage).toContain("textures.minecraft.net/texture/cape");
    expect(cape.style.backgroundSize).toBe("256px 128px");
    expect(cape.style.backgroundPosition).toBe("-48px -4px");
  });
});
