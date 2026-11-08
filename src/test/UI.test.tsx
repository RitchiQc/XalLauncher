import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Button, Modal, ProgressBar, Toggle } from "@/components/UI";

describe("composants UI", () => {
  it("désactive un bouton pendant le chargement", () => {
    render(<Button loading>Installer</Button>);
    expect(screen.getByRole("button", { name: /installer/i })).toBeDisabled();
  });

  it("rend une modale et permet sa fermeture", () => {
    const close = vi.fn();
    render(<Modal title="Nouvelle instance" onClose={close}>Contenu</Modal>);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Fermer" }));
    expect(close).toHaveBeenCalledOnce();
  });

  it("change un interrupteur", () => {
    const change = vi.fn();
    render(<Toggle checked={false} onChange={change} label="Plein écran" />);
    fireEvent.click(screen.getByRole("checkbox"));
    expect(change).toHaveBeenCalledWith(true);
  });

  it("rend une progression accessible visuellement", () => {
    const { container } = render(<ProgressBar value={50} label="Assets" />);
    expect(screen.getByText("50%")).toBeInTheDocument();
    expect(container.querySelector(".progress-track > div")).toHaveStyle({ width: "50%" });
  });
});
