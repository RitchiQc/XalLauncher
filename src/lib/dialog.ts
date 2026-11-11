import { ask } from "@tauri-apps/plugin-dialog";

export type ConfirmKind = "info" | "warning" | "error";

// `window.confirm` est remplacé par le plugin dialog de Tauri et renvoie une Promise :
// un test synchrone `if (!window.confirm(...))` passe donc toujours. On utilise la
// commande native `plugin:dialog|message` via `ask`.
export async function confirmAction(message: string, title = "XalLauncher", kind: ConfirmKind = "warning"): Promise<boolean> {
  try {
    return await ask(message, { title, kind, okLabel: "Oui", cancelLabel: "Non" });
  } catch {
    return window.confirm(message) === true;
  }
}
