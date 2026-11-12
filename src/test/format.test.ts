import { describe, expect, it } from "vitest";
import { formatBytes, formatDuration, formatExportReport, formatImportSummary, formatNumber } from "@/lib/format";
import type { ContentEntry, ImportSummary } from "@/types";

const entry: ContentEntry = { fileName: "a.jar", kind: "mod", enabled: true, size: 1, modifiedAt: "2026-01-01T00:00:00Z" };

describe("formatage local", () => {
  it("formate les tailles binaires", () => {
    expect(formatBytes(0)).toBe("0 o");
    expect(formatBytes(1024)).toBe("1 Ko");
    expect(formatBytes(1024 ** 2)).toBe("1.0 Mo");
  });

  it("formate le temps de jeu", () => {
    expect(formatDuration(42)).toBe("42 s");
    expect(formatDuration(600)).toBe("10 min");
    expect(formatDuration(3900)).toBe("1 h 5 min");
  });

  it("formate les grands nombres", () => {
    expect(formatNumber(12000)).toMatch(/12/);
  });

  it("résume un import multiple", () => {
    const summary: ImportSummary = { imported: [entry, { ...entry, fileName: "b.jar" }], skipped: ["c.jar"], failed: [{ fileName: "d.jar", message: "invalide" }] };
    expect(formatImportSummary(summary)).toBe("2 importés · 1 ignoré · 1 échec");
    expect(formatImportSummary({ imported: [entry], skipped: [], failed: [] })).toBe("1 importé");
  });

  it("résume un export", () => {
    expect(formatExportReport({ path: "pack.mrpack", files: 3, bytes: 2048 })).toBe("3 fichiers · 2 Ko");
  });
});
