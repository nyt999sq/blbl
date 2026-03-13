import test from "node:test";
import assert from "node:assert/strict";
import {
  buildShareSubmittedTask,
  mergeSubmittedShareTasks,
} from "./shareTaskSync.js";

test("buildShareSubmittedTask converts submitted share preset to task item", () => {
  const task = buildShareSubmittedTask({
    id: "preset-1",
    locked_task: {
      project_name: "测试项目",
      screen_name: "第一场",
      sku_name: "VIP",
      count: 1,
    },
    last_submission: {
      submitted_at: 1773295000,
      submitter_uid: "u1",
      submitter_name: "张三",
      task_id: "task-1",
      task_status: "pending",
      buyer_count: 1,
    },
    last_submission_export: {
      ticket_info: {
        buyer_info: [{ id: "buyer-1", name: "张三" }],
      },
      interval: 1000,
      mode: 0,
      total_attempts: 10,
      time_start: "2026-03-12 20:00:00",
      proxy: null,
      time_offset: null,
      ntp_server: "ntp.aliyun.com",
    },
  });

  assert.equal(task.id, "task-1");
  assert.equal(task.project, "测试项目");
  assert.equal(task.status, "pending");
  assert.equal(task.buyers[0].name, "张三");
  assert.equal(task.source, "share_submission");
  assert.equal(task.args.timeStart, "2026-03-12 20:00:00");
});

test("mergeSubmittedShareTasks adds new share tasks and does not duplicate them", () => {
  const presets = [
    {
      id: "preset-1",
      locked_task: {
        project_name: "测试项目",
        screen_name: "第一场",
        sku_name: "VIP",
        count: 1,
      },
      last_submission: {
        submitted_at: 1773295000,
        submitter_uid: "u1",
        submitter_name: "张三",
        task_id: "task-1",
        task_status: "running",
        buyer_count: 1,
      },
      last_submission_export: {
        ticket_info: { buyer_info: [{ id: "buyer-1", name: "张三" }] },
        interval: 1000,
        mode: 0,
        total_attempts: 10,
        time_start: null,
        proxy: null,
        time_offset: null,
        ntp_server: null,
      },
    },
  ];

  const once = mergeSubmittedShareTasks([], presets);
  const twice = mergeSubmittedShareTasks(once, presets);

  assert.equal(once.length, 1);
  assert.equal(twice.length, 1);
  assert.equal(twice[0].status, "running");
});

test("mergeSubmittedShareTasks keeps local terminal state over stale server state", () => {
  const presets = [
    {
      id: "preset-1",
      locked_task: {
        project_name: "测试项目",
        screen_name: "第一场",
        sku_name: "VIP",
        count: 1,
      },
      last_submission: {
        submitted_at: 1773295000,
        submitter_uid: "u1",
        submitter_name: "张三",
        task_id: "task-1",
        task_status: "running",
        buyer_count: 1,
      },
      last_submission_export: {
        ticket_info: { buyer_info: [{ id: "buyer-1", name: "张三" }] },
        interval: 1000,
        mode: 0,
        total_attempts: 10,
        time_start: null,
        proxy: null,
        time_offset: null,
        ntp_server: null,
      },
    },
  ];

  const merged = mergeSubmittedShareTasks(
    [{ id: "task-1", source: "share_submission", status: "success", logs: [], paymentUrl: "" }],
    presets
  );

  assert.equal(merged[0].status, "success");
});
