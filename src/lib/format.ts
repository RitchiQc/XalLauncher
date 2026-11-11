import type { SyntheticEvent } from "react";

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 o";
  const units = ["o", "Ko", "Mo", "Go", "To"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index > 1 ? 1 : 0)} ${units[index]}`;
}

export function formatNumber(value: number): string {
  return new Intl.NumberFormat("fr-FR", { notation: value > 9999 ? "compact" : "standard", maximumFractionDigits: 1 }).format(value);
}

export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return hours ? `${hours} h ${minutes} min` : `${minutes} min`;
}

export function formatDate(value?: string): string {
  if (!value) return "Jamais";
  return new Intl.DateTimeFormat("fr-FR", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
}

export const loaderLabels = { vanilla: "Vanilla", fabric: "Fabric", quilt: "Quilt", forge: "Forge", neoforge: "NeoForge" } as const;

// mc-heads est utilisé en premier : Crafatar répond régulièrement HTTP 500.
export function avatarUrl(uuid: string, size = 128): string {
  return `https://mc-heads.net/avatar/${uuid}/${size}`;
}

export function avatarFallback(uuid: string, size = 128): string {
  return `https://crafatar.com/avatars/${uuid}?size=${size}&overlay=true`;
}

export function handleAvatarError(event: SyntheticEvent<HTMLImageElement>) {
  const image = event.currentTarget;
  const fallback = image.dataset.fallback;
  if (fallback && image.src !== fallback) { image.src = fallback; return; }
  image.style.visibility = "hidden";
}
