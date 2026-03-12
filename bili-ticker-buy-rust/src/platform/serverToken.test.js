import test from "node:test";
import assert from "node:assert/strict";
import { getPreferredServerToken } from "./serverToken.js";

test("prefers env token over stale stored token", () => {
  assert.equal(
    getPreferredServerToken("old-token", "new-token"),
    "new-token"
  );
});

test("uses stored token when env token is missing", () => {
  assert.equal(
    getPreferredServerToken("stored-token", ""),
    "stored-token"
  );
});

test("returns empty string when no token is available", () => {
  assert.equal(getPreferredServerToken("", ""), "");
});
