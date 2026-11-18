import { describe, expect, it } from "vitest";
import config from "../../src-tauri/tauri.conf.json";

describe("configuration Tauri", () => {
  it("autorise l’origine du protocole asset sur Windows dans img-src", () => {
    const { csp } = config.app.security;
    expect(csp).toContain("img-src 'self' asset: http://asset.localhost");
    expect(csp).toContain("https://asset.localhost");
  });

  it("active le protocole asset avec une portée non vide", () => {
    expect(config.app.security.assetProtocol.enable).toBe(true);
    expect(config.app.security.assetProtocol.scope.length).toBeGreaterThan(0);
  });
});
