// The native desktop menu (T7.30, D48). Built in JS so each item dispatches the
// *same* App handler the in-app controls call — one command source, no drift.
// This module is split in two: a pure `appMenuModel` (plain data, unit-tested)
// and a thin `installAppMenu` glue that translates it into Tauri menu objects
// (desktop-only, untestable in jsdom). Web has no native menu; the in-app
// controls and the App's keyboard handlers cover it.
import { isTauri } from "./core";

// Commands the menu dispatches. Each maps to an App handler of the same name;
// `newTemplate` is separate because it carries a template id.
export type MenuCommand =
  | "open"
  | "openFolder"
  | "save"
  | "closeTab"
  | "exportSvg"
  | "exportPng"
  | "exportPdf"
  | "exportBundle"
  | "zoomIn"
  | "zoomOut"
  | "zoomReset"
  | "toggleDock"
  | "help";

// Native items the OS/Tauri implement (no action of ours). Edit verbs act on the
// focused field natively; the macOS app-menu items are the conventional set.
export type PredefinedItem =
  | "Separator"
  | "Copy"
  | "Cut"
  | "Paste"
  | "SelectAll"
  | "Services"
  | "Hide"
  | "HideOthers"
  | "ShowAll"
  | "Quit"
  | "About";

// A node in the pure menu model.
export type MenuNode =
  | { kind: "command"; id: MenuCommand; label: string; accelerator?: string }
  | { kind: "newTemplate"; templateId: string; label: string }
  | { kind: "predefined"; item: PredefinedItem }
  | { kind: "submenu"; label: string; items: MenuNode[] };

export interface MenuModelOptions {
  // macOS gets the leading app-name menu (About/Hide/Quit); other platforms put
  // File first.
  mac: boolean;
  // Templates for the File ▸ New submenu (the in-app New popover's entries).
  templates: readonly { id: string; label: string }[];
}

// Build the menu as plain data. Accelerators are set only on the clean single-key
// commands (Open/Save/Close Tab/Toggle Dock + the desktop folder open); zoom is
// click-only here because its `=`/`+` duality stays with the App's own keyboard
// handler. Exports have no shortcut. Edit verbs are native predefined items —
// Undo/Redo are intentionally omitted so they don't shadow the editor's own
// Cmd+Z history (a native Undo accelerator would never reach CodeMirror).
export function appMenuModel({ mac, templates }: MenuModelOptions): MenuNode[] {
  const menus: MenuNode[] = [];

  if (mac) {
    menus.push({
      kind: "submenu",
      label: "cadtab",
      items: [
        { kind: "predefined", item: "About" },
        { kind: "predefined", item: "Separator" },
        { kind: "predefined", item: "Services" },
        { kind: "predefined", item: "Separator" },
        { kind: "predefined", item: "Hide" },
        { kind: "predefined", item: "HideOthers" },
        { kind: "predefined", item: "ShowAll" },
        { kind: "predefined", item: "Separator" },
        { kind: "predefined", item: "Quit" },
      ],
    });
  }

  menus.push({
    kind: "submenu",
    label: "File",
    items: [
      {
        kind: "submenu",
        label: "New",
        items: templates.map((t) => ({
          kind: "newTemplate",
          templateId: t.id,
          label: t.label,
        })),
      },
      { kind: "predefined", item: "Separator" },
      {
        kind: "command",
        id: "open",
        label: "Open…",
        accelerator: "CmdOrCtrl+O",
      },
      {
        kind: "command",
        id: "openFolder",
        label: "Open Folder…",
        accelerator: "CmdOrCtrl+Shift+O",
      },
      { kind: "predefined", item: "Separator" },
      {
        kind: "command",
        id: "save",
        label: "Save",
        accelerator: "CmdOrCtrl+S",
      },
      { kind: "predefined", item: "Separator" },
      {
        kind: "submenu",
        label: "Export",
        items: [
          { kind: "command", id: "exportSvg", label: "SVG" },
          { kind: "command", id: "exportPng", label: "PNG" },
          { kind: "command", id: "exportPdf", label: "PDF" },
          { kind: "predefined", item: "Separator" },
          { kind: "command", id: "exportBundle", label: "Bundle (.ctabz)" },
        ],
      },
      { kind: "predefined", item: "Separator" },
      {
        kind: "command",
        id: "closeTab",
        label: "Close Tab",
        accelerator: "CmdOrCtrl+W",
      },
    ],
  });

  menus.push({
    kind: "submenu",
    label: "Edit",
    items: [
      { kind: "predefined", item: "Cut" },
      { kind: "predefined", item: "Copy" },
      { kind: "predefined", item: "Paste" },
      { kind: "predefined", item: "SelectAll" },
    ],
  });

  menus.push({
    kind: "submenu",
    label: "View",
    items: [
      { kind: "command", id: "zoomIn", label: "Zoom In" },
      { kind: "command", id: "zoomOut", label: "Zoom Out" },
      { kind: "command", id: "zoomReset", label: "Reset Zoom" },
      { kind: "predefined", item: "Separator" },
      {
        kind: "command",
        id: "toggleDock",
        label: "Toggle Project Dock",
        accelerator: "CmdOrCtrl+B",
      },
    ],
  });

  menus.push({
    kind: "submenu",
    label: "Help",
    items: [{ kind: "command", id: "help", label: "How to Use cadtab" }],
  });

  return menus;
}

// Every command id referenced anywhere in the model, for verifying the handler
// map is exhaustive.
export function menuCommands(nodes: MenuNode[]): MenuCommand[] {
  const out: MenuCommand[] = [];
  const walk = (ns: MenuNode[]) => {
    for (const n of ns) {
      if (n.kind === "command") out.push(n.id);
      else if (n.kind === "submenu") walk(n.items);
    }
  };
  walk(nodes);
  return out;
}

// The App handlers the menu dispatches: one per command, plus the template
// opener. The action closures act on current state, so the menu is built once.
export type MenuHandlers = Record<MenuCommand, () => void> & {
  newTemplate: (id: string) => void;
};

// Returns true on macOS, where the leading app menu (Quit/About) is expected.
export function isMac(): boolean {
  return (
    typeof navigator !== "undefined" && /mac/i.test(navigator.userAgent ?? "")
  );
}

// Build the model and install it as the application menu. Desktop-only and
// best-effort: any failure (e.g. no Tauri runtime under tests) is swallowed, so
// it never breaks the app — the in-app controls remain the functional path.
export async function installAppMenu(
  handlers: MenuHandlers,
  opts: MenuModelOptions,
): Promise<void> {
  if (!isTauri()) return;
  try {
    const { Menu, Submenu, MenuItem, PredefinedMenuItem } =
      await import("@tauri-apps/api/menu");

    type Built = Awaited<
      ReturnType<
        typeof MenuItem.new | typeof Submenu.new | typeof PredefinedMenuItem.new
      >
    >;

    const build = async (node: MenuNode): Promise<Built> => {
      switch (node.kind) {
        case "command":
          return MenuItem.new({
            text: node.label,
            accelerator: node.accelerator,
            action: () => handlers[node.id](),
          });
        case "newTemplate":
          return MenuItem.new({
            text: node.label,
            action: () => handlers.newTemplate(node.templateId),
          });
        case "predefined":
          return PredefinedMenuItem.new({
            item: node.item === "About" ? { About: null } : node.item,
          });
        case "submenu":
          return Submenu.new({
            text: node.label,
            items: await Promise.all(node.items.map(build)),
          });
      }
    };

    const items = await Promise.all(appMenuModel(opts).map(build));
    const menu = await Menu.new({ items });
    await menu.setAsAppMenu();
  } catch {
    // Best-effort: no native menu (or no Tauri runtime) — ignore.
  }
}
