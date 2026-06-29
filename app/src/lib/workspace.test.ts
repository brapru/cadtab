import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
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
  closeTab,
  renameDoc,
  docIdsWithViews,
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
    expect(viewDef("help")?.kind).toBe("global-singleton");
    expect(viewDef("nope")).toBeUndefined();
    expect(Object.keys(VIEWS)).toEqual([
      "editor",
      "render",
      "preview",
      "bottomBar",
      "help",
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

describe("closeTab", () => {
  it("drops a tab and the group it empties", () => {
    const ws = defaultWorkspace("doc"); // g1[editor] | g2[render]
    const next = closeTab(ws, "render:doc");
    expect(next.groups).toHaveLength(1);
    expect(next.groups[0].id).toBe("g1");
    expect(next.groups[0].tabs.map((t) => t.id)).toEqual(["editor:doc"]);
  });

  it("keeps the active tab when a different tab is closed", () => {
    // Stack editor+render in g1, render active; close the (inactive) editor.
    const stacked = moveTab(defaultWorkspace("doc"), "render:doc", "g1");
    expect(stacked.groups[0].activeId).toBe("render:doc");
    const next = closeTab(stacked, "editor:doc");
    expect(next.groups[0].tabs.map((t) => t.id)).toEqual(["render:doc"]);
    expect(next.groups[0].activeId).toBe("render:doc");
  });

  it("falls the active tab back to the first remaining when the active closes", () => {
    const stacked = moveTab(defaultWorkspace("doc"), "render:doc", "g1");
    const next = closeTab(stacked, "render:doc"); // render was active
    expect(next.groups[0].activeId).toBe("editor:doc");
  });

  it("clears a maximize that pointed at the vanished group", () => {
    const ws = toggleMaximize(defaultWorkspace("doc"), "g2");
    const next = closeTab(ws, "render:doc"); // g2 empties and is dropped
    expect(next.maximizedId).toBeNull();
  });

  it("can empty the layout when the last tab closes", () => {
    const lone = moveTab(defaultWorkspace("doc"), "render:doc", "g1");
    const a = closeTab(lone, "render:doc");
    const b = closeTab(a, "editor:doc");
    expect(b.groups).toHaveLength(0);
  });

  it("is a no-op for an unknown instance", () => {
    const ws = defaultWorkspace("doc");
    expect(closeTab(ws, "nope")).toBe(ws);
  });
});

describe("renameDoc", () => {
  it("re-points every view of a doc and follows the active tab", () => {
    // The default layout has editor:a and render:a in two groups, both active.
    const ws = renameDoc(defaultWorkspace("a"), "a", "b");
    expect(docIdsWithViews(ws)).toEqual(new Set(["b"]));
    expect(ws.groups[0].tabs[0]).toEqual(instance("editor", "b"));
    expect(ws.groups[0].activeId).toBe("editor:b");
    expect(ws.groups[1].tabs[0]).toEqual(instance("render", "b"));
    expect(ws.groups[1].activeId).toBe("render:b");
  });

  it("leaves other docs' tabs untouched", () => {
    let ws = defaultWorkspace("a");
    ws = addTab(ws, instance("editor", "keep"), "g1");
    ws = renameDoc(ws, "a", "b");
    expect(docIdsWithViews(ws)).toEqual(new Set(["b", "keep"]));
  });
});

describe("docIdsWithViews", () => {
  it("collects the doc ids that still have an open view", () => {
    let ws = defaultWorkspace("a");
    ws = addTab(ws, instance("editor", "b"), "g1");
    expect([...docIdsWithViews(ws)].sort()).toEqual(["a", "b"]);
    // Closing a's render leaves its editor, so a still counts...
    expect(docIdsWithViews(closeTab(ws, "render:a")).has("a")).toBe(true);
    // ...but closing both of a's views drops it.
    const closed = closeTab(closeTab(ws, "render:a"), "editor:a");
    expect(docIdsWithViews(closed).has("a")).toBe(false);
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

  it("labels tabs by filename when a docName resolver is supplied (icon carries the view type)", () => {
    // D49: every view of one file shares its filename; the registry title is
    // only a fallback. Both the editor and render of "doc" read "tune.ctab".
    const { container } = render(Workspace, {
      workspace: defaultWorkspace("doc"),
      view: stubView,
      docName: () => "tune.ctab",
    });
    expect(
      [...container.querySelectorAll(".tab-title")].map((t) => t.textContent),
    ).toEqual(["tune.ctab", "tune.ctab"]);
  });

  it("falls back to the registry title when the resolver yields no name", () => {
    // An unsaved draft (resolver returns null) keeps the view's registry title.
    const { container } = render(Workspace, {
      workspace: defaultWorkspace("doc"),
      view: stubView,
      docName: () => null,
    });
    expect(
      [...container.querySelectorAll(".tab-title")].map((t) => t.textContent),
    ).toEqual(["Editor", "Render"]);
  });

  it("strikes the missing-on-disk tab on its filename label", () => {
    // The missing strike rides the filename now that labels are filenames.
    const { container } = render(Workspace, {
      workspace: defaultWorkspace("doc"),
      view: stubView,
      docName: () => "tune.ctab",
      missingDocIds: ["doc"],
    });
    const struck = [...container.querySelectorAll(".tab-title.missing")];
    expect(struck.map((t) => t.textContent)).toEqual([
      "tune.ctab",
      "tune.ctab",
    ]);
  });

  it("maximizes a group, hiding the other and its gutter, then restores", async () => {
    const { container, getByLabelText } = mountShell();

    // Controls live on the active group; activate the render group first so its
    // maximize control is the one shown.
    await fireEvent.pointerDown(container.querySelectorAll(".group")[1]);
    await fireEvent.click(getByLabelText("Maximize group"));
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

  it("puts a render launcher in the control set when the active tab is an editor", async () => {
    const onOpenRender = vi.fn();
    // Editor active, render closed: the control invites opening it.
    const { getByLabelText, queryByLabelText } = render(Workspace, {
      workspace: closeTab(defaultWorkspace("doc"), "render:doc"), // g1 [editor]
      view: stubView,
      onOpenRender,
    });
    await fireEvent.click(getByLabelText("Open render"));
    expect(onOpenRender).toHaveBeenCalledWith("doc");
    // No "Go to render" while the render is closed.
    expect(queryByLabelText("Go to render")).toBeNull();
  });

  it("marks the render launcher as jump-to when the render is already open", () => {
    const { getByLabelText, container } = render(Workspace, {
      workspace: defaultWorkspace("doc"), // editor active (g1), render open (g2)
      view: stubView,
    });
    // The active editor's control set shows a jump-to render launcher.
    expect(getByLabelText("Go to render")).toBeTruthy();
    expect(container.querySelector(".launch.open")).not.toBeNull();
    // Only the active group's control set carries it.
    expect(container.querySelectorAll(".launch")).toHaveLength(1);
  });

  it("opens the New template menu and reports the chosen template", async () => {
    const onNew = vi.fn();
    const { getAllByLabelText, getByText, container } = render(Workspace, {
      workspace: defaultWorkspace("doc"),
      view: stubView,
      onNew,
      newTemplates: [
        { id: "banjo", label: "Banjo" },
        { id: "blank", label: "Blank" },
      ],
    });
    // No menu until the "+" is clicked.
    expect(container.querySelector(".new-menu")).toBeNull();
    await fireEvent.click(getAllByLabelText("New tab")[0]);
    expect(container.querySelector(".new-menu")).not.toBeNull();

    // Picking a template reports it and closes the menu.
    await fireEvent.click(getByText("Blank"));
    expect(onNew).toHaveBeenCalledWith("blank");
    expect(container.querySelector(".new-menu")).toBeNull();
  });

  it("dismisses the New menu on Escape", async () => {
    const { getAllByLabelText, container } = render(Workspace, {
      workspace: defaultWorkspace("doc"),
      view: stubView,
      newTemplates: [{ id: "blank", label: "Blank" }],
    });
    await fireEvent.click(getAllByLabelText("New tab")[0]);
    expect(container.querySelector(".new-menu")).not.toBeNull();
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(container.querySelector(".new-menu")).toBeNull();
  });

  it("dismisses the New menu on a pointer down outside it", async () => {
    const { getAllByLabelText, container } = render(Workspace, {
      workspace: defaultWorkspace("doc"),
      view: stubView,
      newTemplates: [{ id: "blank", label: "Blank" }],
    });
    await fireEvent.click(getAllByLabelText("New tab")[0]);
    expect(container.querySelector(".new-menu")).not.toBeNull();
    // A press outside the New control closes it...
    await fireEvent.pointerDown(document.body);
    expect(container.querySelector(".new-menu")).toBeNull();
    // ...but a press inside it keeps it open.
    await fireEvent.click(getAllByLabelText("New tab")[0]);
    await fireEvent.pointerDown(container.querySelector(".new-menu")!);
    expect(container.querySelector(".new-menu")).not.toBeNull();
  });

  it("keeps a New control reachable in the empty-tabs placeholder", async () => {
    const onNew = vi.fn();
    const { container, getByLabelText, getByText } = render(Workspace, {
      workspace: { groups: [], maximizedId: null },
      view: stubView,
      onNew,
      newTemplates: [{ id: "blank", label: "Blank" }],
    });
    expect(container.querySelector(".empty")).not.toBeNull();
    await fireEvent.click(getByLabelText("New tab"));
    await fireEvent.click(getByText("Blank"));
    expect(onNew).toHaveBeenCalledWith("blank");
  });

  it("maximizes a group by double-clicking its tab, and restores on a second", async () => {
    const { container } = mountShell();
    const editorTab = () =>
      [...container.querySelectorAll<HTMLElement>(".tab")].find((t) =>
        t.textContent?.includes("Editor"),
      )!;
    // Double-clicking the editor tab maximizes its group (the render hides).
    await fireEvent.dblClick(editorTab());
    expect(container.querySelectorAll(".group")).toHaveLength(1);
    expect(container.querySelector(".stub")?.textContent).toBe("editor");
    // A second double-click on the maximized tab restores the row.
    await fireEvent.dblClick(editorTab());
    expect(container.querySelectorAll(".group")).toHaveLength(2);
  });

  it("offers Fit only on the active group when it shows a render", async () => {
    const onFit = vi.fn();
    const { container, getByLabelText } = render(Workspace, {
      workspace: defaultWorkspace("doc"), // g1 editor | g2 render
      view: stubView,
      onFit,
    });
    // The editor group is active by default — no Fit there.
    expect(container.querySelectorAll(".fit")).toHaveLength(0);
    // Activating the render group reveals Fit, which reports the request.
    await fireEvent.pointerDown(container.querySelectorAll(".group")[1]);
    expect(container.querySelectorAll(".fit")).toHaveLength(1);
    await fireEvent.click(getByLabelText("Fit to width"));
    expect(onFit).toHaveBeenCalledOnce();
  });

  it("shows a close affordance on every tab that reports the instance closed", async () => {
    const onCloseTab = vi.fn();
    const { container } = render(Workspace, {
      workspace: defaultWorkspace("doc"),
      view: stubView,
      onCloseTab,
    });
    // Every tab shares the uniform "Close tab" affordance; the close button is
    // found via its tab's icon ("code" editor, "music_note" render).
    const closeFor = (icon: string) =>
      [...container.querySelectorAll(".tab-wrap")]
        .find((w) => w.querySelector(".tab-icon")?.textContent === icon)!
        .querySelector(".tab-close")!;
    await fireEvent.click(closeFor("code"));
    expect(onCloseTab).toHaveBeenCalledWith(
      expect.objectContaining({ id: "editor:doc", type: "editor" }),
    );
    await fireEvent.click(closeFor("music_note"));
    expect(onCloseTab).toHaveBeenCalledWith(
      expect.objectContaining({ id: "render:doc", type: "render" }),
    );
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
    const { container, getByLabelText } = mountShell();
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
    // Activate the popped (weight-0.5) group so its maximize control shows.
    await fireEvent.pointerDown(container.querySelectorAll(".group")[1]);
    await fireEvent.click(getByLabelText("Maximize group"));
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

  it("cues only the target group's open tab space while dragging a tab over it", async () => {
    const { container } = mountShell();
    const groups = container.querySelectorAll(".group");
    (groups[0] as HTMLElement).getBoundingClientRect = rect(0, 100);
    (groups[1] as HTMLElement).getBoundingClientRect = rect(100, 200);
    const renderTab = [...container.querySelectorAll(".tab")].find((t) =>
      t.textContent?.includes("Render"),
    )!;
    // Press on the render tab (g2 at x150) and drag left over the editor group.
    ptr(renderTab, "pointerdown", 150);
    ptr(renderTab, "pointermove", 50);
    await tick();

    // Only the hovered group's open drop space is highlighted — not the other
    // group, not the strip behind the tabs, the view body, or the whole group.
    const zones = container.querySelectorAll(".dropzone");
    expect(zones[0].classList.contains("droptarget")).toBe(true);
    expect(zones[1].classList.contains("droptarget")).toBe(false);
    expect(container.querySelector(".tabstrip.droptarget")).toBeNull();
    expect(container.querySelector(".group-body.droptarget")).toBeNull();
    expect(container.querySelector(".group.droptarget")).toBeNull();

    ptr(renderTab, "pointerup", 50);
  });
});
