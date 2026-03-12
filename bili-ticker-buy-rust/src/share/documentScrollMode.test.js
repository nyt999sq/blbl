import test from "node:test";
import assert from "node:assert/strict";
import { enableDocumentScroll } from "./documentScrollMode.js";

function createMockElement(initial = {}) {
  return {
    style: {
      overflow: initial.overflow || "",
      height: initial.height || "",
    },
  };
}

test("enableDocumentScroll overrides page-level hidden overflow and restores it", () => {
  const body = createMockElement({ overflow: "hidden", height: "100vh" });
  const html = createMockElement({ overflow: "", height: "100%" });
  const root = createMockElement({ height: "100%" });
  const documentRef = {
    body,
    documentElement: html,
    getElementById(id) {
      return id === "root" ? root : null;
    },
  };

  const cleanup = enableDocumentScroll(documentRef);

  assert.equal(body.style.overflow, "auto");
  assert.equal(body.style.height, "auto");
  assert.equal(html.style.overflow, "auto");
  assert.equal(html.style.height, "auto");
  assert.equal(root.style.height, "auto");

  cleanup();

  assert.equal(body.style.overflow, "hidden");
  assert.equal(body.style.height, "100vh");
  assert.equal(html.style.overflow, "");
  assert.equal(html.style.height, "100%");
  assert.equal(root.style.height, "100%");
});
