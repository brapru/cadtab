import { describe, it, expect } from "vitest";
import {
  newSession,
  singleDocStore,
  isDirty,
  activeDoc,
  putDoc,
  setActiveContent,
  setDocContent,
  setActive,
  markActiveSaved,
  removeDoc,
  reloadDoc,
  markMissingOnDisk,
} from "./documents";

describe("newSession", () => {
  it("seeds the dirty baseline from the initial content (saved by default)", () => {
    const d = newSession("doc", { content: "score {}" });
    expect(d).toEqual({
      id: "doc",
      name: null,
      path: null,
      content: "score {}",
      savedContent: "score {}",
      everSaved: true,
      missingOnDisk: false,
    });
    expect(isDirty(d)).toBe(false);
  });

  it("a never-saved draft is dirty from birth", () => {
    const d = newSession("draft:1", { content: "x", everSaved: false });
    expect(d.everSaved).toBe(false);
    expect(isDirty(d)).toBe(true);
  });
});

describe("isDirty", () => {
  it("is dirty only while the buffer diverges from the baseline", () => {
    const d = newSession("doc", { content: "a" });
    expect(isDirty({ ...d, content: "b" })).toBe(true);
    expect(isDirty({ ...d, content: "a" })).toBe(false);
  });

  it("stays dirty for a never-saved draft even at its baseline content", () => {
    const d = newSession("draft:1", { content: "a", everSaved: false });
    expect(isDirty(d)).toBe(true);
    expect(isDirty({ ...d, content: "a" })).toBe(true);
  });
});

describe("active document", () => {
  it("tracks the focused session", () => {
    const store = singleDocStore(newSession("doc", { content: "x" }));
    expect(activeDoc(store)?.id).toBe("doc");
    expect(activeDoc({ docs: store.docs, activeId: null })).toBeNull();
  });
});

describe("putDoc", () => {
  it("replaces the same id in place and focuses it", () => {
    const store = singleDocStore(newSession("doc", { content: "old" }));
    const next = putDoc(
      store,
      newSession("doc", { content: "new", name: "t" }),
    );
    expect(next.docs).toHaveLength(1);
    expect(activeDoc(next)?.content).toBe("new");
    expect(activeDoc(next)?.name).toBe("t");
  });

  it("appends a new id as another open document, focused", () => {
    const store = singleDocStore(newSession("a", { content: "1" }));
    const next = putDoc(store, newSession("b", { content: "2" }));
    expect(next.docs.map((d) => d.id)).toEqual(["a", "b"]);
    expect(next.activeId).toBe("b");
  });
});

describe("setActiveContent", () => {
  it("updates the active buffer, leaving the baseline (so dirty derives)", () => {
    const store = singleDocStore(newSession("doc", { content: "a" }));
    const edited = setActiveContent(store, "ab");
    const doc = activeDoc(edited)!;
    expect(doc.content).toBe("ab");
    expect(doc.savedContent).toBe("a");
    expect(isDirty(doc)).toBe(true);
  });
});

describe("setDocContent / setActive", () => {
  it("edits a specific document, not just the active one", () => {
    let store = putDoc(
      singleDocStore(newSession("a", { content: "1" })),
      newSession("b", { content: "2" }),
    );
    // active is "b"; edit "a" by id.
    store = setDocContent(store, "a", "1!");
    expect(store.docs.find((d) => d.id === "a")?.content).toBe("1!");
    expect(store.activeId).toBe("b");
  });

  it("focuses an open document and ignores an unknown id", () => {
    const store = putDoc(
      singleDocStore(newSession("a", { content: "1" })),
      newSession("b", { content: "2" }),
    );
    expect(setActive(store, "a").activeId).toBe("a");
    expect(setActive(store, "ghost")).toBe(store);
  });
});

describe("removeDoc", () => {
  it("drops a document and moves focus to the last remaining one", () => {
    let store = putDoc(
      singleDocStore(newSession("a", { content: "1" })),
      newSession("b", { content: "2" }),
    ); // active = b
    store = removeDoc(store, "b");
    expect(store.docs.map((d) => d.id)).toEqual(["a"]);
    expect(store.activeId).toBe("a");
  });

  it("keeps the current focus when a non-active doc is removed", () => {
    let store = putDoc(
      singleDocStore(newSession("a", { content: "1" })),
      newSession("b", { content: "2" }),
    );
    store = setActive(store, "a"); // active = a
    store = removeDoc(store, "b");
    expect(store.activeId).toBe("a");
  });

  it("nulls focus when the last document closes", () => {
    const store = removeDoc(
      singleDocStore(newSession("a", { content: "1" })),
      "a",
    );
    expect(store.docs).toEqual([]);
    expect(store.activeId).toBeNull();
  });

  it("is a no-op for an unknown id", () => {
    const store = singleDocStore(newSession("a", { content: "1" }));
    expect(removeDoc(store, "ghost")).toBe(store);
  });
});

describe("markActiveSaved", () => {
  it("rebaselines to the current content and adopts the saved path/name", () => {
    let store = singleDocStore(newSession("doc", { content: "a" }));
    store = setActiveContent(store, "ab");
    store = markActiveSaved(store, { path: "/x.ctab", name: "x.ctab" });
    const doc = activeDoc(store)!;
    expect(isDirty(doc)).toBe(false);
    expect(doc.savedContent).toBe("ab");
    expect(doc.path).toBe("/x.ctab");
    expect(doc.name).toBe("x.ctab");
  });

  it("clears the never-saved flag on a draft's first save", () => {
    let store = singleDocStore(
      newSession("draft:1", { content: "a", everSaved: false }),
    );
    expect(isDirty(activeDoc(store)!)).toBe(true);
    store = markActiveSaved(store, { path: null, name: "a.ctab" });
    expect(isDirty(activeDoc(store)!)).toBe(false);
  });
});

describe("reloadDoc", () => {
  it("replaces a doc's buffer from disk and rebaselines it clean", () => {
    let store = singleDocStore(
      newSession("file:tune.ctab", { content: "OLD" }),
    );
    store = setActiveContent(store, "edited"); // dirty
    expect(isDirty(activeDoc(store)!)).toBe(true);
    store = reloadDoc(store, "file:tune.ctab", "DISK");
    const doc = activeDoc(store)!;
    expect(doc.content).toBe("DISK");
    expect(doc.savedContent).toBe("DISK");
    expect(isDirty(doc)).toBe(false);
  });

  it("is a no-op for an id that isn't open", () => {
    const store = singleDocStore(newSession("file:a.ctab", { content: "a" }));
    expect(reloadDoc(store, "file:ghost.ctab", "x")).toEqual(store);
  });
});

describe("markMissingOnDisk", () => {
  it("flags file docs whose key the scan dropped, leaving drafts alone", () => {
    let store = putDoc(
      singleDocStore(newSession("file:tune.ctab", { content: "a" })),
      newSession("draft:1", { content: "b", everSaved: false }),
    );
    const present = new Set(["other.ctab"]); // tune.ctab is gone
    store = markMissingOnDisk(store, (key) => !present.has(key));
    expect(
      store.docs.find((d) => d.id === "file:tune.ctab")?.missingOnDisk,
    ).toBe(true);
    // Drafts have no on-disk identity, so they're untouched.
    expect(store.docs.find((d) => d.id === "draft:1")?.missingOnDisk).toBe(
      false,
    );
  });

  it("clears the flag when the file reappears, and keeps unchanged docs by reference", () => {
    let store = singleDocStore(newSession("file:tune.ctab", { content: "a" }));
    const before = store.docs[0];
    // Still present → no change → same object reference (no reactive churn).
    store = markMissingOnDisk(store, () => false);
    expect(store.docs[0]).toBe(before);
    // Now missing, then present again.
    store = markMissingOnDisk(store, () => true);
    expect(store.docs[0].missingOnDisk).toBe(true);
    store = markMissingOnDisk(store, () => false);
    expect(store.docs[0].missingOnDisk).toBe(false);
  });
});
