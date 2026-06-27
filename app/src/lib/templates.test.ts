import { describe, it, expect } from "vitest";
import { TEMPLATES, templateById } from "./templates";

describe("templates", () => {
  it("offers banjo, guitar, and blank starters", () => {
    expect(TEMPLATES.map((t) => t.id)).toEqual(["banjo", "guitar", "blank"]);
    for (const t of TEMPLATES) {
      expect(t.label).toBeTruthy();
      expect(t.source).toContain("score");
    }
  });

  it("the instrument templates declare their instrument", () => {
    expect(templateById("banjo")?.source).toContain("instrument banjo");
    expect(templateById("guitar")?.source).toContain("instrument guitar");
  });

  it("looks up by id and returns undefined for an unknown id", () => {
    expect(templateById("banjo")?.id).toBe("banjo");
    expect(templateById("nope")).toBeUndefined();
  });
});
