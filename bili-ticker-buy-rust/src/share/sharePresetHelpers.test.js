import test from "node:test";
import assert from "node:assert/strict";
import {
  formatDateToLocalDateTime,
  normalizeDateTimeLocalValue,
  normalizeOptionalString,
} from "./sharePresetHelpers.js";

test("normalizeOptionalString coerces numeric ids to strings", () => {
  assert.equal(normalizeOptionalString(123), "123");
  assert.equal(normalizeOptionalString("  abc  "), "abc");
  assert.equal(normalizeOptionalString("   "), null);
});

test("normalizeDateTimeLocalValue pads date pieces for datetime-local inputs", () => {
  assert.equal(
    normalizeDateTimeLocalValue("2026-2-22 18:00:00"),
    "2026-02-22T18:00:00"
  );
});

test("formatDateToLocalDateTime returns canonical local datetime string", () => {
  const date = new Date(2026, 1, 22, 18, 0, 5);
  assert.equal(
    formatDateToLocalDateTime(date, true),
    "2026-02-22 18:00:05"
  );
});
