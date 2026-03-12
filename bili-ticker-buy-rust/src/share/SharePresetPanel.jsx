import { useEffect, useState } from "react";
import { Copy, Download, Link2, Lock, RefreshCw, ShieldCheck, XCircle } from "lucide-react";
import {
  batchDeleteSharePresets,
  closeSharePreset,
  createSharePreset,
  exportSharePresetConfig,
  listSharePresets,
} from "../platform/apiClient";
import { copyText } from "../utils/clipboard";
import { normalizeOptionalString } from "./sharePresetHelpers";

const statusLabelMap = {
  active: "可用",
  completed: "已使用",
  expired: "已过期",
  closed: "已关闭",
};

const statusClassMap = {
  active: "bg-green-500/15 text-green-300 border-green-500/30",
  completed: "bg-blue-500/15 text-blue-300 border-blue-500/30",
  expired: "bg-yellow-500/15 text-yellow-300 border-yellow-500/30",
  closed: "bg-red-500/15 text-red-300 border-red-500/30",
};

function formatTime(ts) {
  if (!ts) return "未设置";
  return new Date(ts * 1000).toLocaleString("zh-CN", { hour12: false });
}

export default function SharePresetPanel({
  enabled,
  lockedTask,
  displaySnapshot,
  creatorName,
  creatorUid,
  ticketCount,
  setTicketCount,
}) {
  const [title, setTitle] = useState("");
  const [expiresHours, setExpiresHours] = useState("24");
  const [presets, setPresets] = useState([]);
  const [latestLinks, setLatestLinks] = useState({});
  const [loading, setLoading] = useState(false);
  const [selectedPresetIds, setSelectedPresetIds] = useState([]);

  const deletableStatuses = new Set(["completed", "expired", "closed"]);

  const loadPresets = async () => {
    if (!enabled) return;
    try {
      const result = await listSharePresets();
      const nextPresets = Array.isArray(result) ? result : [];
      setPresets(nextPresets);
      setSelectedPresetIds((prev) =>
        prev.filter((id) => nextPresets.some((preset) => preset.id === id))
      );
    } catch (error) {
      console.error("load share presets failed", error);
    }
  };

  useEffect(() => {
    loadPresets();
  }, [enabled]);

  const handleCreate = async () => {
    if (!enabled) {
      alert("分享链接仅支持 headless Web 模式");
      return;
    }
    if (!lockedTask || !displaySnapshot) {
      alert("请先选择项目、场次和票档");
      return;
    }

    setLoading(true);
    try {
      const expiresAt =
        expiresHours === "0"
          ? null
          : Math.floor(Date.now() / 1000) + Number(expiresHours) * 3600;
      const result = await createSharePreset({
        locked_task: lockedTask,
        display_snapshot: displaySnapshot,
        expires_at: expiresAt,
        title: normalizeOptionalString(title),
        creator_uid: normalizeOptionalString(creatorUid),
        creator_name: normalizeOptionalString(creatorName),
        base_url: window.location.origin,
      });
      setLatestLinks((prev) => ({
        ...prev,
        [result.preset_id]: result.share_url,
      }));
      await loadPresets();
      const copied = await copyText(result.share_url);
      alert(copied ? "分享链接已生成并复制到剪贴板" : "分享链接已生成，请按提示手动复制");
    } catch (error) {
      alert(`生成分享链接失败: ${error.message || error}`);
    } finally {
      setLoading(false);
    }
  };

  const handleClose = async (presetId) => {
    if (!confirm("确定要关闭这条分享链接吗？关闭后不可恢复。")) return;
    try {
      await closeSharePreset(presetId);
      await loadPresets();
    } catch (error) {
      alert(`关闭失败: ${error.message || error}`);
    }
  };

  const togglePresetSelection = (presetId) => {
    setSelectedPresetIds((prev) =>
      prev.includes(presetId)
        ? prev.filter((id) => id !== presetId)
        : [...prev, presetId]
    );
  };

  const selectAllDeletable = () => {
    setSelectedPresetIds(
      presets
        .filter((preset) => deletableStatuses.has(preset.status))
        .map((preset) => preset.id)
    );
  };

  const handleBatchDelete = async () => {
    if (selectedPresetIds.length === 0) {
      alert("请至少选择一条已使用或已失效的分享链接");
      return;
    }
    if (!confirm(`确定要删除选中的 ${selectedPresetIds.length} 条分享链接吗？`)) {
      return;
    }
    try {
      await batchDeleteSharePresets(selectedPresetIds);
      setLatestLinks((prev) =>
        Object.fromEntries(
          Object.entries(prev).filter(([presetId]) => !selectedPresetIds.includes(presetId))
        )
      );
      setSelectedPresetIds([]);
      await loadPresets();
      alert("批量删除成功");
    } catch (error) {
      alert(`批量删除失败: ${error.message || error}`);
    }
  };

  const handleExportConfig = async (preset) => {
    try {
      const config = await exportSharePresetConfig(preset.id);
      const blob = new Blob([JSON.stringify(config, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `share-submission-${preset.id}.json`;
      link.click();
      URL.revokeObjectURL(url);
    } catch (error) {
      alert(`导出配置失败: ${error.message || error}`);
    }
  };

  return (
    <div className="mt-6 rounded-xl border border-gray-700 bg-gray-900/60 p-5 space-y-5">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h4 className="text-lg font-bold flex items-center gap-2">
            <Link2 size={18} className="text-cyan-400" />
            分享抢票配置链接
          </h4>
          <p className="text-xs text-gray-400 mt-1">
            生成后，对方只能填写自己的实名、联系人与登录授权，不能改票。
          </p>
        </div>
        <button
          onClick={loadPresets}
          className="px-3 py-2 text-sm rounded-lg border border-gray-700 bg-gray-800 hover:bg-gray-700 text-gray-200 flex items-center gap-2"
        >
          <RefreshCw size={14} />
          刷新
        </button>
      </div>

      {!enabled && (
        <div className="rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-4 py-3 text-sm text-yellow-200">
          当前为桌面 Tauri 运行时，无法生成可对外访问的 headless Web 分享链接。
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <div className="rounded-lg border border-gray-800 bg-black/20 p-4 space-y-3">
          <div className="text-sm font-semibold text-gray-200 flex items-center gap-2">
            <Lock size={14} className="text-cyan-400" />
            锁定内容
          </div>
          <div className="text-xs text-gray-400 space-y-2">
            <div>项目：{lockedTask?.project_name || "未选择"}</div>
            <div>场次：{lockedTask?.screen_name || "未选择"}</div>
            <div>票档：{lockedTask?.sku_name || "未选择"}</div>
            <div>开抢时间：{lockedTask?.time_start || "立即抢票"}</div>
            <div>策略：{lockedTask ? `${lockedTask.interval}ms / ${lockedTask.mode === 0 ? "无限循环" : `有限尝试 ${lockedTask.total_attempts} 次`}` : "未配置"}</div>
          </div>
        </div>

        <div className="rounded-lg border border-gray-800 bg-black/20 p-4 space-y-3">
          <div className="text-sm font-semibold text-gray-200 flex items-center gap-2">
            <ShieldCheck size={14} className="text-cyan-400" />
            分享设置
          </div>
          <div className="space-y-3">
            <div>
              <label className="block text-xs text-gray-400 mb-1">链接标题</label>
              <input
                type="text"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-cyan-500"
                placeholder="例如：周六晚场双人票"
              />
            </div>
            <div>
              <label className="block text-xs text-gray-400 mb-1">锁定张数</label>
              <input
                type="number"
                min="1"
                max="6"
                value={ticketCount}
                onChange={(e) => setTicketCount(Math.max(1, Number(e.target.value) || 1))}
                className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-cyan-500"
              />
            </div>
            <div>
              <label className="block text-xs text-gray-400 mb-1">有效期</label>
              <select
                value={expiresHours}
                onChange={(e) => setExpiresHours(e.target.value)}
                className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-cyan-500"
              >
                <option value="24">24 小时</option>
                <option value="72">72 小时</option>
                <option value="168">7 天</option>
                <option value="0">不过期</option>
              </select>
            </div>
          </div>
        </div>

        <div className="rounded-lg border border-gray-800 bg-black/20 p-4 space-y-3">
          <div className="text-sm font-semibold text-gray-200">生成规则</div>
          <div className="text-xs text-gray-400 space-y-2">
            <div>1. 链接默认只允许成功提交一次。</div>
            <div>2. 提交后立即创建任务，若设置了时间则进入等待。</div>
            <div>3. 出于安全考虑，原始链接仅在创建当次可再次复制。</div>
          </div>
          <button
            onClick={handleCreate}
            disabled={!enabled || !lockedTask || loading}
            className={`w-full px-4 py-2 rounded-lg font-semibold text-sm ${
              !enabled || !lockedTask || loading
                ? "bg-gray-700 text-gray-400 cursor-not-allowed"
                : "bg-cyan-600 hover:bg-cyan-500 text-white"
            }`}
          >
            {loading ? "生成中..." : "生成分享链接"}
          </button>
        </div>
      </div>

      <div className="space-y-3">
        <div className="flex items-center justify-between gap-3">
          <div className="text-xs text-gray-500">
            可批量删除已完成、已过期、已关闭的分享链接。
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={selectAllDeletable}
              className="px-3 py-2 rounded-lg border border-gray-700 bg-gray-800 hover:bg-gray-700 text-xs text-gray-200"
            >
              全选可删除项
            </button>
            <button
              onClick={handleBatchDelete}
              disabled={selectedPresetIds.length === 0}
              className={`px-3 py-2 rounded-lg text-xs font-semibold ${
                selectedPresetIds.length === 0
                  ? "bg-gray-700 text-gray-400 cursor-not-allowed"
                  : "bg-red-600 hover:bg-red-500 text-white"
              }`}
            >
              批量删除已使用链接
            </button>
          </div>
        </div>
        {presets.length === 0 && (
          <div className="rounded-lg border border-dashed border-gray-700 bg-black/10 px-4 py-6 text-sm text-gray-500 text-center">
            暂无分享链接
          </div>
        )}
        {presets.map((preset) => (
          <div
            key={preset.id}
            className="rounded-lg border border-gray-800 bg-black/20 px-4 py-4 flex flex-col gap-3"
          >
            <div className="flex items-start justify-between gap-4">
              <div className="space-y-1 flex-1">
                <div className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={selectedPresetIds.includes(preset.id)}
                    disabled={!deletableStatuses.has(preset.status)}
                    onChange={() => togglePresetSelection(preset.id)}
                    className="accent-cyan-500"
                  />
                  <span className="font-semibold text-white">
                    {preset.title || `${preset.locked_task.project_name} / ${preset.locked_task.sku_name}`}
                  </span>
                  <span
                    className={`px-2 py-0.5 rounded-full text-[11px] border ${
                      statusClassMap[preset.status] || statusClassMap.closed
                    }`}
                  >
                    {statusLabelMap[preset.status] || preset.status}
                  </span>
                </div>
                <div className="text-xs text-gray-400">
                  {preset.locked_task.screen_name} / {preset.locked_task.sku_name} / {preset.locked_task.count} 张
                </div>
                <div className="text-xs text-gray-500">
                  创建于 {formatTime(preset.created_at)}，到期 {formatTime(preset.expires_at)}
                </div>
                {preset.last_submission && (
                  <div className="text-xs text-gray-400">
                    最近提交：{preset.last_submission.submitter_name} 于{" "}
                    {formatTime(preset.last_submission.submitted_at)}，任务 {preset.last_submission.task_status}
                  </div>
                )}
              </div>

              <div className="flex items-center gap-2">
                {preset.has_export_config && (
                  <button
                    onClick={() => handleExportConfig(preset)}
                    className="px-3 py-2 rounded-lg border border-gray-700 bg-gray-800 hover:bg-gray-700 text-sm text-white flex items-center gap-2"
                  >
                    <Download size={14} />
                    导出配置
                  </button>
                )}
                {latestLinks[preset.id] && (
                  <button
                    onClick={async () => {
                      const copied = await copyText(latestLinks[preset.id]);
                      if (!copied) {
                        alert("当前浏览器不支持自动复制，请按提示手动复制");
                      }
                    }}
                    className="px-3 py-2 rounded-lg border border-gray-700 bg-gray-800 hover:bg-gray-700 text-sm text-white flex items-center gap-2"
                  >
                    <Copy size={14} />
                    复制链接
                  </button>
                )}
                {preset.status === "active" && (
                  <button
                    onClick={() => handleClose(preset.id)}
                    className="px-3 py-2 rounded-lg border border-red-500/30 bg-red-500/10 hover:bg-red-500/20 text-sm text-red-300 flex items-center gap-2"
                  >
                    <XCircle size={14} />
                    关闭
                  </button>
                )}
              </div>
            </div>
            {!latestLinks[preset.id] && (
              <div className="text-xs text-gray-500">
                原始分享链接仅在本次创建后保留于当前页面；如需重新分发，建议重新生成一条新链接。
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
