import { check } from "@tauri-apps/plugin-updater";

export async function findUpdate() {
  return check();
}
