import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import { createRawSnippet } from "svelte";
import Workspace from "./Workspace.svelte";
import {
  VIEWS,
  viewDef,
  instance,
  defaultWorkspace,
  activeTab,
  pairRatio,
  activateTab,
  toggleMaximize,
  resizePair,
  type ViewInstance,
} from "./workspace";

describe("view registry", () => {
  it("classes the built-in views by kind", () => {
    expect(viewDef("editor")?.kind).toBe("document-bound");
    expect(viewDef("render")?.kind).toBe("document-bound");
    expect(viewDef("nope")).toBeUndefined();
    expect(Object.keys(VIEWS)).toEqual(["editor", "render"]);
  });
});

describe("instance", () => {
  it("derives a stable id per (type, doc); singletons drop the doc", () => {
    expect(instance("editor", "a")).toEqual({
      id: "editor:a",
      type: "editor",
      docId: "a",
    });
    expect(instance("dock")).toEqual({ id: "dock", type: "dock", docId: null });
  });
});

describe("defaultWorkspace", () => {
  it("is the editor|render split as two one-tab groups", () => {
    const ws = defaultWorkspace("doc");
    expect(ws.groups.map((g) => g.tabs.map((t) => t.type))).toEqual([
      ["editor"],
      ["render"],
    ]);
    expect(ws.groups.map((g) => g.activeId)).toEqual([
      "editor:doc",
      "render:doc",
    ]);
    expect(ws.maximizedId).toBeNull();
  });
});

describe("activeTab", () => {
  it("returns the active tab, falling back to the first", () => {
    const ws = defaultWorkspace("doc");
    expect(activeTab(ws.groups[0])?.type).toBe("editor");
    const orphaned = { ...ws.groups[0], activeId: "missing" };
    expect(activeTab(orphaned)?.type).toBe("editor");
  });
});

describe("activateTab", () => {
  it("sets the active tab only for a tab the group holds", () => {
    const ws = defaultWorkspace("doc");
    const same = activateTab(ws, "g1", "render:doc"); // not in g1
    expect(same.groups[0].activeId).toBe("editor:doc");
    // (With one tab per group there is nothing else to switch to; the guard is
    // what matters — foreign ids never take.)
  });
});

describe("toggleMaximize", () => {
  it("maximizes a group then restores on the second toggle", () => {
    const ws = defaultWorkspace("doc");
    const max = toggleMaximize(ws, "g2");
    expect(max.maximizedId).toBe("g2");
    expect(toggleMaximize(max, "g2").maximizedId).toBeNull();
  });
});

describe("resizePair / pairRatio", () => {
  it("splits the pair's combined weight by the ratio", () => {
    const ws = defaultWorkspace("doc");
    expect(pairRatio(ws, 0)).toBe(0.5);
    const wide = resizePair(ws, 0, 0.7);
    expect(pairRatio(wide, 0)).toBeCloseTo(0.7);
    expect(wide.groups[0].weight + wide.groups[1].weight).toBe(2);
  });

  it("clamps so neither group can be dragged shut", () => {
    const ws = defaultWorkspace("doc");
    expect(pairRatio(resizePair(ws, 0, 5), 0)).toBeCloseTo(0.85);
    expect(pairRatio(resizePair(ws, 0, -5), 0)).toBeCloseTo(0.15);
  });

  it("leaves the workspace untouched for an out-of-range boundary", () => {
    const ws = defaultWorkspace("doc");
    expect(resizePair(ws, 5, 0.7)).toBe(ws);
  });
});

// A minimal stand-in for the parent's view snippet: render the tab's type, so we
// can assert which view a group shows without pulling in Editor/Tab.
const stubView = createRawSnippet((instance: () => ViewInstance) => ({
  render: () => `<div class="stub">${instance().type}</div>`,
}));

function mountShell() {
  return render(Workspace, {
    workspace: defaultWorkspace("doc"),
    view: stubView,
  });
}

describe("Workspace chrome", () => {
  it("renders a group per tab with the registry titles and a gutter between", () => {
    const { container } = mountShell();
    expect(
      [...container.querySelectorAll(".tab-title")].map((t) => t.textContent),
    ).toEqual(["Editor", "Render"]);
    expect(container.querySelectorAll(".group")).toHaveLength(2);
    expect(container.querySelectorAll(".gutter")).toHaveLength(1);
    // Each group mounts its active view through the snippet.
    expect(
      [...container.querySelectorAll(".stub")].map((s) => s.textContent),
    ).toEqual(["editor", "render"]);
  });

  it("maximizes a group, hiding the other and its gutter, then restores", async () => {
    const { container, getAllByLabelText, getByLabelText } = mountShell();

    await fireEvent.click(getAllByLabelText("Maximize group")[1]);
    expect(container.querySelectorAll(".group")).toHaveLength(1);
    expect(container.querySelectorAll(".gutter")).toHaveLength(0);
    expect(container.querySelector(".stub")?.textContent).toBe("render");

    await fireEvent.click(getByLabelText("Restore group"));
    expect(container.querySelectorAll(".group")).toHaveLength(2);
    expect(container.querySelectorAll(".gutter")).toHaveLength(1);
  });

  it("marks the active tab in each group", () => {
    const { container } = mountShell();
    expect(
      [...container.querySelectorAll(".tab.active")].map(
        (t) => t.querySelector(".tab-title")?.textContent,
      ),
    ).toEqual(["Editor", "Render"]);
  });
});
