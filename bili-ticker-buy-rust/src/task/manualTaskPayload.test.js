import test from "node:test";
import assert from "node:assert/strict";
import { buildManualTicketInfo } from "./manualTaskPayload.js";

test("buildManualTicketInfo透传热票标记而不是写死为false", () => {
  const result = buildManualTicketInfo({
    projectId: "project-1",
    projectName: "测试项目",
    selectedScreen: {
      id: 100,
      name: "夜场",
      screen_type: 2,
    },
    selectedSku: {
      id: 200,
      desc: "VIP",
      price: 6800,
      is_hot_project: false,
    },
    sanitizedBuyers: [{ id: "buyer-1", name: "Alice", tel: "13800000000" }],
    cookies: ["SESSDATA=abc"],
    topDeliverInfo: {},
    topName: "Alice",
    topTel: "13800000000",
  });

  assert.equal(result.is_hot_project, true);
});
