import test from "node:test";
import assert from "node:assert/strict";
import { copyText } from "./clipboard.js";

test("copyText uses clipboard api when available", async () => {
  let copied = null;
  const result = await copyText("share-link", {
    navigatorRef: {
      clipboard: {
        writeText: async (value) => {
          copied = value;
        },
      },
    },
    documentRef: undefined,
    windowRef: undefined,
  });

  assert.equal(result, true);
  assert.equal(copied, "share-link");
});

test("copyText falls back to execCommand copy", async () => {
  let appended = false;
  let removed = false;
  const textarea = {
    value: "",
    style: {},
    setAttribute() {},
    focus() {},
    select() {},
    setSelectionRange() {},
  };
  const result = await copyText("legacy-copy", {
    navigatorRef: {},
    documentRef: {
      createElement: () => textarea,
      body: {
        appendChild(node) {
          appended = node === textarea;
        },
        removeChild(node) {
          removed = node === textarea;
        },
      },
      execCommand(command) {
        return command === "copy";
      },
    },
    windowRef: undefined,
  });

  assert.equal(result, true);
  assert.equal(appended, true);
  assert.equal(removed, true);
});

test("copyText falls back to prompt when no copy api is available", async () => {
  let prompted = null;
  const result = await copyText("manual-copy", {
    navigatorRef: {},
    documentRef: {
      createElement() {
        throw new Error("no dom copy");
      },
    },
    windowRef: {
      prompt(message, value) {
        prompted = { message, value };
      },
    },
  });

  assert.equal(result, false);
  assert.deepEqual(prompted, {
    message: "当前浏览器不支持自动复制，请手动复制以下内容：",
    value: "manual-copy",
  });
});
