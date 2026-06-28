// The workspace layout model (D41): a registry of views placed into editor
// groups. Pure data + operations, so the shell chrome stays thin and this is
// testable in isolation. No free-floating docking — the area is a row of groups,
// each a stack of tabs with one active, resizable and individually maximizable.
import { clampSplit } from "./split";

// Global singletons exist once and are not tied to a document (the project dock,
// the bottom bar); document-bound views belong to a specific `.ctab` (editor,
// render, print preview, later the looper), so "file A + its render" and "file B
// + its render" can coexist in different groups.
export type ViewKind = "global-singleton" | "document-bound";

// A registered view type: what the shell needs to render a tab for it. The
// component that actually mounts is resolved by the shell (kept out of this
// module so the model stays pure), keyed by `type`.
export interface ViewDef {
  type: string;
  title: string;
  icon: string;
  kind: ViewKind;
}

// The view registry: every tool the shell knows. Document-bound views are placed
// into groups as tabs (the shell looks them up here for a title/icon);
// global-singleton views (the bottom bar, later the project dock) are mounted
// once as chrome, not tabbed. New tool = new entry here + its mount point.
export const VIEWS: Record<string, ViewDef> = {
  editor: {
    type: "editor",
    title: "Editor",
    icon: "✎",
    kind: "document-bound",
  },
  render: {
    type: "render",
    title: "Render",
    icon: "♪",
    kind: "document-bound",
  },
  bottomBar: {
    type: "bottomBar",
    title: "Status Bar",
    icon: "▭",
    kind: "global-singleton",
  },
};

export function viewDef(type: string): ViewDef | undefined {
  return VIEWS[type];
}

// An open instance of a view: one tab in a group. Document-bound instances carry
// the doc they belong to; singletons leave it null. The id is derived so the
// same (type, doc) pair always maps to a stable tab (no duplicate editors for
// one file).
export interface ViewInstance {
  id: string;
  type: string;
  docId: string | null;
}

export function instance(
  type: string,
  docId: string | null = null,
): ViewInstance {
  return { id: docId === null ? type : `${type}:${docId}`, type, docId };
}

// A group: a pane holding a stack of tabs with one active, plus a flex `weight`
// governing its share of the row width.
export interface Group {
  id: string;
  tabs: ViewInstance[];
  activeId: string | null;
  weight: number;
}

// The editor-groups layout: a row of groups, optionally one maximized ("zoomed")
// so it fills the area while the others stay in the model.
export interface Workspace {
  groups: Group[];
  maximizedId: string | null;
}

function group(id: string, tabs: ViewInstance[], weight = 1): Group {
  return { id, tabs, activeId: tabs[0]?.id ?? null, weight };
}

// The starting layout: editor on the left, render on the right — today's split,
// expressed as the N=2 / one-tab-each case of the groups model.
export function defaultWorkspace(docId: string): Workspace {
  return {
    groups: [
      group("g1", [instance("editor", docId)]),
      group("g2", [instance("render", docId)]),
    ],
    maximizedId: null,
  };
}

// The active tab of a group, falling back to its first tab so a group always
// shows something while it holds tabs.
export function activeTab(g: Group): ViewInstance | null {
  return g.tabs.find((t) => t.id === g.activeId) ?? g.tabs[0] ?? null;
}

// The fraction of a pair's combined weight held by the left group — what a gutter
// between groups `i` and `i+1` reflects and adjusts.
export function pairRatio(ws: Workspace, leftIndex: number): number {
  const a = ws.groups[leftIndex];
  const b = ws.groups[leftIndex + 1];
  if (!a || !b) return 0.5;
  const total = a.weight + b.weight;
  return total <= 0 ? 0.5 : a.weight / total;
}

export function activateTab(
  ws: Workspace,
  groupId: string,
  instanceId: string,
): Workspace {
  return {
    ...ws,
    groups: ws.groups.map((g) =>
      g.id === groupId && g.tabs.some((t) => t.id === instanceId)
        ? { ...g, activeId: instanceId }
        : g,
    ),
  };
}

// Maximize a group, or restore if it is already maximized — the "zoom" toggle.
export function toggleMaximize(ws: Workspace, groupId: string): Workspace {
  return { ...ws, maximizedId: ws.maximizedId === groupId ? null : groupId };
}

// Resize the boundary between group `leftIndex` and the next: split their
// combined weight by `ratio` (clamped so neither closes), leaving the rest
// untouched. For the N=2 case this is the old editor|render split ratio.
export function resizePair(
  ws: Workspace,
  leftIndex: number,
  ratio: number,
): Workspace {
  const a = ws.groups[leftIndex];
  const b = ws.groups[leftIndex + 1];
  if (!a || !b) return ws;
  const total = a.weight + b.weight;
  const r = clampSplit(ratio);
  const groups = ws.groups.slice();
  groups[leftIndex] = { ...a, weight: total * r };
  groups[leftIndex + 1] = { ...b, weight: total * (1 - r) };
  return { ...ws, groups };
}

// Move a tab into another group, appending it there and giving it focus. It
// leaves its source group (whose active tab falls back to the first remaining),
// and an emptied source group is dropped — so dragging the render onto the
// editor's group stacks them, collapsing back to one group. No-op when the tab
// is already in the target, or the target no longer exists.
export function moveTab(
  ws: Workspace,
  instanceId: string,
  toGroupId: string,
): Workspace {
  const from = ws.groups.find((g) => g.tabs.some((t) => t.id === instanceId));
  if (!from || from.id === toGroupId) return ws;
  if (!ws.groups.some((g) => g.id === toGroupId)) return ws;
  const moved = from.tabs.find((t) => t.id === instanceId)!;
  const groups = ws.groups
    .map((g) => {
      if (g.id === from.id) {
        const tabs = g.tabs.filter((t) => t.id !== instanceId);
        return { ...g, tabs, activeId: tabs[0]?.id ?? null };
      }
      if (g.id === toGroupId) {
        return { ...g, tabs: [...g.tabs, moved], activeId: moved.id };
      }
      return g;
    })
    .filter((g) => g.tabs.length > 0);
  // A maximized group that just vanished shouldn't strand the layout.
  const maximizedId =
    ws.maximizedId && groups.some((g) => g.id === ws.maximizedId)
      ? ws.maximizedId
      : null;
  return { groups, maximizedId };
}

// The id of the first group holding a tab of `type` (any document), or null —
// used to route a newly opened file's editor/render tabs next to the existing
// ones.
export function groupOfType(ws: Workspace, type: string): string | null {
  return ws.groups.find((g) => g.tabs.some((t) => t.type === type))?.id ?? null;
}

// Add a tab to `toGroupId` and focus it. Idempotent: if the instance is already
// open in any group, that group just focuses it (no duplicate) — so re-opening a
// file activates its existing tab. No-op if the target group is gone.
export function addTab(
  ws: Workspace,
  inst: ViewInstance,
  toGroupId: string,
): Workspace {
  const existing = ws.groups.find((g) => g.tabs.some((t) => t.id === inst.id));
  if (existing) {
    return {
      ...ws,
      groups: ws.groups.map((g) =>
        g.id === existing.id ? { ...g, activeId: inst.id } : g,
      ),
    };
  }
  if (!ws.groups.some((g) => g.id === toGroupId)) return ws;
  return {
    ...ws,
    groups: ws.groups.map((g) =>
      g.id === toGroupId
        ? { ...g, tabs: [...g.tabs, inst], activeId: inst.id }
        : g,
    ),
  };
}

// Pop a group's active tab into a fresh group inserted just after it — the
// "split" verb, halving the source's width for the new pane. No-op unless the
// group has more than one tab (a lone tab has nothing to split off).
export function splitTab(ws: Workspace, groupId: string): Workspace {
  const index = ws.groups.findIndex((g) => g.id === groupId);
  const src = ws.groups[index];
  if (!src || src.tabs.length <= 1) return ws;
  const active = activeTab(src)!;
  const remaining = src.tabs.filter((t) => t.id !== active.id);
  const half = src.weight / 2;
  const shrunk: Group = {
    ...src,
    tabs: remaining,
    activeId: remaining[0]?.id ?? null,
    weight: half,
  };
  const popped: Group = {
    id: `g-${active.id}`,
    tabs: [active],
    activeId: active.id,
    weight: half,
  };
  const groups = ws.groups.slice();
  groups.splice(index, 1, shrunk, popped);
  return { ...ws, groups };
}
