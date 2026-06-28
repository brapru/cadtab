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
} from "./documents";

describe("newSession", () => {
  it("seeds the dirty baseline from the initial content", () => {
    const d = newSession("doc", { content: "score {}" });
    expect(d).toEqual({
      id: "doc",
      name: null,
      path: null,
      content: "score {}",
      savedContent: "score {}",
    });
    expect(isDirty(d)).toBe(false);
  });
});

describe("isDirty", () => {
  it("is dirty only while the buffer diverges from the baseline", () => {
    const d = newSession("doc", { content: "a" });
    expect(isDirty({ ...d, content: "b" })).toBe(true);
    expect(isDirty({ ...d, content: "a" })).toBe(false);
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
});
