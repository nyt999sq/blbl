function formatDateTime(ts) {
  if (!ts) return "-";
  return new Date(ts * 1000).toLocaleString("zh-CN", { hour12: false });
}

function normalizeTaskStatus(taskStatus) {
  if (taskStatus === "pending") return "pending";
  if (taskStatus === "scheduled") return "scheduled";
  if (taskStatus === "running") return "running";
  if (taskStatus === "success") return "success";
  if (taskStatus === "stopped" || taskStatus === "failed") return "stopped";
  return "running";
}

export function buildShareSubmittedTask(preset) {
  if (!preset?.last_submission?.task_id) return null;
  if (!preset?.last_submission_export) return null;

  const submitterName =
    preset.last_submission.submitter_name || "分享提交用户";
  const exportConfig = preset.last_submission_export;
  const buyers = Array.isArray(exportConfig?.ticket_info?.buyer_info)
    ? exportConfig.ticket_info.buyer_info
    : [{ id: preset.last_submission.submitter_uid || preset.id, name: submitterName }];

  return {
    id: preset.last_submission.task_id,
    project: preset.locked_task?.project_name || "分享任务",
    screen: preset.locked_task?.screen_name || "未知场次",
    sku: preset.locked_task?.sku_name || "未知票档",
    buyerCount: preset.last_submission.buyer_count || preset.locked_task?.count || 0,
    buyers,
    startTime: formatDateTime(preset.last_submission.submitted_at),
    status: normalizeTaskStatus(preset.last_submission.task_status),
    logs: [],
    lastLog: `分享链接提交：${submitterName}`,
    paymentUrl: "",
    accountName: "分享提交",
    args: {
      taskId: preset.last_submission.task_id,
      ticketInfo: JSON.stringify(exportConfig.ticket_info),
      interval: exportConfig.interval,
      mode: exportConfig.mode,
      totalAttempts: exportConfig.total_attempts,
      timeStart: exportConfig.time_start,
      proxy: exportConfig.proxy,
      timeOffset: exportConfig.time_offset,
      buyers,
      ntpServer: exportConfig.ntp_server,
    },
    source: "share_submission",
    sharePresetId: preset.id,
  };
}

export function mergeSubmittedShareTasks(existingTasks, presets) {
  const nextTasks = [...existingTasks];
  const taskIndexMap = new Map(nextTasks.map((task, index) => [task.id, index]));

  for (const preset of presets || []) {
    const shareTask = buildShareSubmittedTask(preset);
    if (!shareTask) continue;

    const existingIndex = taskIndexMap.get(shareTask.id);
    if (existingIndex === undefined) {
      nextTasks.unshift(shareTask);
      taskIndexMap.set(shareTask.id, 0);
      continue;
    }

    const current = nextTasks[existingIndex];
    if (current?.source === "share_submission") {
      const preservedTerminalStatus =
        current.status === "success" || current.status === "stopped"
          ? current.status
          : null;
      nextTasks[existingIndex] = {
        ...current,
        ...shareTask,
        status: preservedTerminalStatus || shareTask.status,
        logs: current.logs || [],
        paymentUrl: current.paymentUrl || shareTask.paymentUrl,
      };
    }
  }

  return nextTasks;
}
