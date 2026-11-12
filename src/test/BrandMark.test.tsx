import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BrandMark } from "@/components/BrandMark";

describe("BrandMark", () => {
  it("affiche le logo sans texte alternatif redondant", () => {
    const { container } = render(<BrandMark />);
    const image = container.querySelector("img.brand-mark");
    expect(image).toBeInTheDocument();
    expect(image).toHaveAttribute("alt", "");
  });

  it("agrandit le logo dans l'écran de chargement", () => {
    const { container } = render(<BrandMark large />);
    expect(container.querySelector("img.brand-mark.large")).toBeInTheDocument();
  });
});
