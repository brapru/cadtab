import { describe, it, expect } from "vitest";
import { serializeBundle, parseBundle, type ProjectBundle } from "./bundle";
// The shipped example project + its generated bundle, as raw text.
import exampleBundle from "../../../examples/cripple-creek.ctabz?raw";
import exampleEntry from "../../../examples/cripple-creek-project/cripple-creek.ctab?raw";
import exampleLicks from "../../../examples/cripple-creek-project/licks.ctab?raw";

const sample: ProjectBundle = {
  entry: "tune.ctab",
  files: {
    "tune.ctab": 'import "rolls.ctab"\nscore { my_roll([3:0 2:0 1:0]) }',
    "rolls.ctab": "def my_roll(c) { c.0 .t }",
  },
};

describe("project bundle", () => {
  it("round-trips through serialize/parse", () => {
    const back = parseBundle(serializeBundle(sample));
    expect(back).toEqual(sample);
  });

  it("writes a versioned envelope", () => {
    const json = JSON.parse(serializeBundle(sample));
    expect(json.version).toBe(1);
    expect(json.entry).toBe("tune.ctab");
    expect(Object.keys(json.files)).toEqual(["tune.ctab", "rolls.ctab"]);
  });

  it("rejects invalid JSON", () => {
    expect(() => parseBundle("{not json")).toThrow(/invalid JSON/);
  });

  it("rejects a non-object payload", () => {
    expect(() => parseBundle("42")).toThrow(/not a valid project bundle/);
  });

  it("requires a string entry", () => {
    expect(() => parseBundle('{"files":{}}')).toThrow(
      /missing a string .entry/,
    );
  });

  it("requires a files map", () => {
    expect(() => parseBundle('{"entry":"a.ctab"}')).toThrow(
      /missing a .files. map/,
    );
  });

  it("requires file contents to be text", () => {
    const json = '{"entry":"a.ctab","files":{"a.ctab":123}}';
    expect(() => parseBundle(json)).toThrow(/"a.ctab" is not text/);
  });

  it("requires the entry to be present among the files", () => {
    const json = '{"entry":"missing.ctab","files":{"a.ctab":"score {}"}}';
    expect(() => parseBundle(json)).toThrow(
      /entry "missing.ctab" is not among/,
    );
  });

  it("rejects a newer bundle version", () => {
    const json =
      '{"version":99,"entry":"a.ctab","files":{"a.ctab":"score {}"}}';
    expect(() => parseBundle(json)).toThrow(/newer than this app supports/);
  });

  // Guards the shipped example bundle: it must parse, and its files must match
  // the on-disk project they were generated from (so the two never drift).
  it("the example .ctabz parses and matches the project files", () => {
    const bundle = parseBundle(exampleBundle);
    expect(bundle.entry).toBe("cripple-creek.ctab");
    expect(Object.keys(bundle.files).sort()).toEqual([
      "cripple-creek.ctab",
      "licks.ctab",
    ]);
    expect(bundle.files["cripple-creek.ctab"]).toBe(exampleEntry);
    expect(bundle.files["licks.ctab"]).toBe(exampleLicks);
  });
});
