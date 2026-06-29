import { describe, it, expect } from "vitest";
import {
  appMenuModel,
  menuCommands,
  type MenuNode,
  type MenuCommand,
} from "./menu";

const templates = [
  { id: "banjo", label: "Banjo (open G)" },
  { id: "blank", label: "Blank" },
];

function submenu(nodes: MenuNode[], label: string): MenuNode[] {
  const found = nodes.find(
    (n): n is Extract<MenuNode, { kind: "submenu" }> =>
      n.kind === "submenu" && n.label === label,
  );
  if (!found) throw new Error(`no submenu "${label}"`);
  return found.items;
}

function command(nodes: MenuNode[], id: MenuCommand) {
  const found = nodes.find(
    (n): n is Extract<MenuNode, { kind: "command" }> =>
      n.kind === "command" && n.id === id,
  );
  if (!found) throw new Error(`no command "${id}"`);
  return found;
}

function predefinedItems(nodes: MenuNode[]): string[] {
  return nodes.filter((n) => n.kind === "predefined").map((n) => n.item);
}

describe("appMenuModel", () => {
  it("leads with the app menu on macOS (About…Quit)", () => {
    const model = appMenuModel({ mac: true, templates });
    expect(model[0]).toMatchObject({ kind: "submenu", label: "cadtab" });
    const app = submenu(model, "cadtab");
    expect(predefinedItems(app)).toContain("Quit");
    expect(predefinedItems(app)).toContain("About");
  });

  it("leads with File off macOS (no app menu)", () => {
    const model = appMenuModel({ mac: false, templates });
    expect(model[0]).toMatchObject({ kind: "submenu", label: "File" });
    expect(
      model.some((n) => n.kind === "submenu" && n.label === "cadtab"),
    ).toBe(false);
  });

  it("builds File with New templates, accelerated open/save/close, and Export", () => {
    const file = submenu(appMenuModel({ mac: true, templates }), "File");

    // New ▸ <templates> mirrors the in-app New popover entries.
    const newItems = submenu(file, "New");
    expect(newItems).toEqual([
      { kind: "newTemplate", templateId: "banjo", label: "Banjo (open G)" },
      { kind: "newTemplate", templateId: "blank", label: "Blank" },
    ]);

    expect(command(file, "open").accelerator).toBe("CmdOrCtrl+O");
    expect(command(file, "openFolder").accelerator).toBe("CmdOrCtrl+Shift+O");
    expect(command(file, "save").accelerator).toBe("CmdOrCtrl+S");
    expect(command(file, "closeTab").accelerator).toBe("CmdOrCtrl+W");

    const exp = submenu(file, "Export");
    expect(exp.filter((n) => n.kind === "command").map((n) => n.id)).toEqual([
      "exportSvg",
      "exportPng",
      "exportPdf",
      "exportBundle",
    ]);
  });

  it("View zoom items are click-only; Toggle Dock keeps its accelerator", () => {
    const view = submenu(appMenuModel({ mac: true, templates }), "View");
    expect(command(view, "zoomIn").accelerator).toBeUndefined();
    expect(command(view, "zoomOut").accelerator).toBeUndefined();
    expect(command(view, "zoomReset").accelerator).toBeUndefined();
    expect(command(view, "toggleDock").accelerator).toBe("CmdOrCtrl+B");
  });

  it("Edit exposes the native clipboard verbs but not Undo/Redo", () => {
    const edit = submenu(appMenuModel({ mac: true, templates }), "Edit");
    expect(predefinedItems(edit)).toEqual([
      "Cut",
      "Copy",
      "Paste",
      "SelectAll",
    ]);
  });

  it("covers every command exactly once across the tree", () => {
    const cmds = menuCommands(appMenuModel({ mac: true, templates })).sort();
    expect(cmds).toEqual(
      [
        "open",
        "openFolder",
        "save",
        "closeTab",
        "exportSvg",
        "exportPng",
        "exportPdf",
        "exportBundle",
        "zoomIn",
        "zoomOut",
        "zoomReset",
        "toggleDock",
        "help",
      ].sort(),
    );
  });
});
