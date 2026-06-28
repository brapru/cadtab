import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import { createRawSnippet, tick } from "svelte";
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
  moveTab,
  splitTab,
  groupOfType,
  addTab,
  type ViewInstance,
} from "./workspace";

describe("view registry", () => {
  it("classes the built-in views by kind", () => {
    expect(viewDef("editor")?.kind).toBe("document-bound");
    expect(viewDef("render")?.kind).toBe("document-bound");
    expect(viewDef("preview")?.kind).toBe("document-bound");
    expect(viewDef("bottomBar")?.kind).toBe("global-singleton");
    expect(viewDef("nope")).toBeUndefined();
    expect(Object.keys(VIEWS)).toEqual([
      "editor",
      "render",
      "preview",
      "bottomBar",
    ]);
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

describe("moveTab", () => {
  it("stacks the render onto the editor group and drops the emptied group", () => {
    const ws = defaultWorkspace("doc");
    const moved = moveTab(ws, "render:doc", "g1");
    expect(moved.groups).toHaveLength(1);
    expect(moved.groups[0].id).toBe("g1");
    expect(moved.groups[0].tabs.map((t) => t.type)).toEqual([
      "editor",
      "render",
    ]);
    // The moved tab takes focus in its new group.
    expect(moved.groups[0].activeId).toBe("render:doc");
  });

  it("repairs the source group's active tab when the active one leaves", () => {
    // Put both tabs in g1, active = render, then move render back out to g2...
    const stacked = moveTab(defaultWorkspace("doc"), "render:doc", "g1");
    // g1 now [editor, render] active render; move editor elsewhere isn't possible
    // (g2 gone), so verify the simpler case: source active falls back.
    const split = splitTab(stacked, "g1"); // pops render into its own group
    const g1 = split.groups.find((g) =>
      g.tabs.some((t) => t.type === "editor"),
    );
    expect(g1?.activeId).toBe("editor:doc");
  });

  it("is a no-op moving to the same group or a missing target", () => {
    const ws = defaultWorkspace("doc");
    expect(moveTab(ws, "editor:doc", "g1")).toBe(ws);
    expect(moveTab(ws, "editor:doc", "nope")).toBe(ws);
  });

  it("clears a maximize that pointed at the vanished group", () => {
    const ws = toggleMaximize(defaultWorkspace("doc"), "g2");
    const moved = moveTab(ws, "render:doc", "g1"); // g2 disappears
    expect(moved.maximizedId).toBeNull();
  });
});

describe("splitTab", () => {
  it("pops the active tab into a new group, halving the source weight", () => {
    const stacked = moveTab(defaultWorkspace("doc"), "render:doc", "g1");
    expect(stacked.groups).toHaveLength(1);
    const split = splitTab(stacked, "g1");
    expect(split.groups).toHaveLength(2);
    expect(split.groups.map((g) => g.tabs.map((t) => t.type))).toEqual([
      ["editor"],
      ["render"],
    ]);
    expect(split.groups[1].id).toBe("g-render:doc");
    expect(split.groups[0].weight).toBe(0.5);
    expect(split.groups[1].weight).toBe(0.5);
  });

  it("is a no-op for a lone tab or unknown group", () => {
    const ws = defaultWorkspace("doc");
    expect(splitTab(ws, "g1")).toBe(ws); // g1 has only the editor
    expect(splitTab(ws, "nope")).toBe(ws);
  });
});

describe("groupOfType", () => {
  it("finds the group hosting a view type, else null", () => {
    const ws = defaultWorkspace("doc");
    expect(groupOfType(ws, "editor")).toBe("g1");
    expect(groupOfType(ws, "render")).toBe("g2");
    expect(groupOfType(ws, "preview")).toBeNull();
  });
});

describe("addTab", () => {
  it("appends a second document's editor into the editor group, focused", () => {
    const ws = defaultWorkspace("a");
    const next = addTab(ws, instance("editor", "b"), "g1");
    expect(next.groups[0].tabs.map((t) => t.id)).toEqual([
      "editor:a",
      "editor:b",
    ]);
    expect(next.groups[0].activeId).toBe("editor:b");
  });

  it("re-focuses an already-open tab instead of duplicating it", () => {
    const ws = defaultWorkspace("a");
    const next = addTab(ws, instance("render", "a"), "g1"); // render:a already in g2
    expect(next.groups[1].activeId).toBe("render:a");
    // No duplicate landed in g1.
    expect(next.groups[0].tabs.map((t) => t.id)).toEqual(["editor:a"]);
  });

  it("is a no-op for a missing target group", () => {
    const ws = defaultWorkspace("a");
    expect(addTab(ws, instance("editor", "b"), "nope")).toBe(ws);
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

  // jsdom doesn't deliver clientX on synthetic pointer events, so dispatch a
  // MouseEvent (which carries clientX) typed as the pointer event; pointerId is
  // attached for the (guarded) pointer-capture call.
  function ptr(el: Element, type: string, clientX: number) {
    const e = new MouseEvent(type, { bubbles: true, button: 0, clientX });
    Object.defineProperty(e, "pointerId", { value: 1 });
    el.dispatchEvent(e);
  }
  function rect(left: number, right: number) {
    return () =>
      ({
        left,
        right,
        top: 0,
        bottom: 300,
        width: right - left,
        height: 300,
      }) as DOMRect;
  }

  it("drags the render tab (pointer events) onto the editor group, then splits back", async () => {
    const { container } = mountShell();
    // Give the two groups distinct horizontal extents so the pointer drag can
    // hit-test which one it's over (jsdom has no layout of its own).
    const groups = container.querySelectorAll(".group");
    (groups[0] as HTMLElement).getBoundingClientRect = rect(0, 100); // editor (g1)
    (groups[1] as HTMLElement).getBoundingClientRect = rect(100, 200); // render (g2)

    const renderTab = [...container.querySelectorAll(".tab")].find((t) =>
      t.textContent?.includes("Render"),
    )!;

    // No split control while every group holds a single tab.
    expect(container.querySelector(".split")).toBeNull();

    // Press on the render tab (in g2) and drag left into the editor group (g1).
    // (Manual dispatch is synchronous, so flush Svelte's update with tick.)
    ptr(renderTab, "pointerdown", 150);
    ptr(renderTab, "pointermove", 50);
    ptr(renderTab, "pointerup", 50);
    await tick();

    // One group now stacks both tabs (render focused), the other is gone.
    expect(container.querySelectorAll(".group")).toHaveLength(1);
    expect(
      [...container.querySelectorAll(".tab-title")].map((t) => t.textContent),
    ).toEqual(["Editor", "Render"]);

    // The split control appears and pops the active tab back into its own group.
    const split = container.querySelector(".split")!;
    await fireEvent.click(split);
    expect(container.querySelectorAll(".group")).toHaveLength(2);
    expect(container.querySelectorAll(".gutter")).toHaveLength(1);
  });

  // Pull the numeric flex-grow each group renders with. The shell normalizes raw
  // model weights over the visible groups, so these always sum to ~1 and the row
  // fills regardless of the weight churn that produced them.
  function flexGrows(container: Element): number[] {
    return [...container.querySelectorAll<HTMLElement>(".group")].map((g) =>
      parseFloat(g.style.flex),
    );
  }

  it("normalizes flex so the row fills after move→split→move churn", async () => {
    const { container } = mountShell();
    const groups = container.querySelectorAll(".group");
    (groups[0] as HTMLElement).getBoundingClientRect = rect(0, 100);
    (groups[1] as HTMLElement).getBoundingClientRect = rect(100, 200);
    const renderTab = () =>
      [...container.querySelectorAll(".tab")].find((t) =>
        t.textContent?.includes("Render"),
      )!;

    // Two even groups fill the row to start.
    expect(flexGrows(container)).toEqual([0.5, 0.5]);

    // Move the render onto the editor group (stack), collapsing to one group.
    ptr(renderTab(), "pointerdown", 150);
    ptr(renderTab(), "pointermove", 50);
    ptr(renderTab(), "pointerup", 50);
    await tick();
    // A lone group fills regardless of its raw weight.
    expect(flexGrows(container)).toEqual([1]);

    // Split the render back out — raw weights are now 0.5/0.5 (sum 0.5).
    await fireEvent.click(container.querySelector(".split")!);
    expect(flexGrows(container)).toEqual([0.5, 0.5]);

    // Move it back onto the editor group: one group with raw weight 0.5. Without
    // normalization its flex-grow would be 0.5 and the view would be cut off; the
    // shell renders a full-filling 1 instead.
    const g = container.querySelectorAll(".group");
    (g[0] as HTMLElement).getBoundingClientRect = rect(0, 100);
    (g[1] as HTMLElement).getBoundingClientRect = rect(100, 200);
    ptr(renderTab(), "pointerdown", 150);
    ptr(renderTab(), "pointermove", 50);
    ptr(renderTab(), "pointerup", 50);
    await tick();
    expect(flexGrows(container)).toEqual([1]);
  });

  it("fills a maximized sub-1-weight group", async () => {
    const { container, getAllByLabelText } = mountShell();
    const groups = container.querySelectorAll(".group");
    (groups[0] as HTMLElement).getBoundingClientRect = rect(0, 100);
    (groups[1] as HTMLElement).getBoundingClientRect = rect(100, 200);
    // Stack then split so a group carries weight 0.5, then maximize it alone.
    const renderTab = () =>
      [...container.querySelectorAll(".tab")].find((t) =>
        t.textContent?.includes("Render"),
      )!;
    ptr(renderTab(), "pointerdown", 150);
    ptr(renderTab(), "pointermove", 50);
    ptr(renderTab(), "pointerup", 50);
    await tick();
    await fireEvent.click(container.querySelector(".split")!);
    await fireEvent.click(getAllByLabelText("Maximize group")[1]);
    // Only the weight-0.5 group shows, and it fills.
    expect(flexGrows(container)).toEqual([1]);
  });

  it("treats a pointer press without movement as a click, not a drag", () => {
    const { container } = mountShell();
    const groups = container.querySelectorAll(".group");
    (groups[0] as HTMLElement).getBoundingClientRect = rect(0, 100);
    (groups[1] as HTMLElement).getBoundingClientRect = rect(100, 200);
    const renderTab = [...container.querySelectorAll(".tab")].find((t) =>
      t.textContent?.includes("Render"),
    )!;
    // A press-release in place (no movement) leaves the layout untouched.
    ptr(renderTab, "pointerdown", 150);
    ptr(renderTab, "pointerup", 150);
    expect(container.querySelectorAll(".group")).toHaveLength(2);
  });
});
