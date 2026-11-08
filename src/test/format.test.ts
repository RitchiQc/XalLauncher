import { describe, expect, it } from "vitest";
import { formatBytes, formatDuration, formatNumber } from "@/lib/format";

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
});
