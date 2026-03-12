import { useState } from "react";
import { KeyRound, ShieldCheck } from "lucide-react";
import logo from "./assets/logo.png";
import { clearAdminAuth, getStoredAdminToken, loginWithAdminToken } from "./platform/apiClient";

export default function AdminTokenGatePage({ onAuthenticated }) {
  const [token, setToken] = useState(() => getStoredAdminToken());
  const [hasSavedToken, setHasSavedToken] = useState(() => getStoredAdminToken().trim().length > 0);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  const handleSubmit = async (event) => {
    event.preventDefault();
    if (!token.trim()) {
      setError("请输入后台访问 token");
      return;
    }

    setSubmitting(true);
    setError("");
    try {
      await loginWithAdminToken(token);
      onAuthenticated();
    } catch (submitError) {
      setError("token 验证失败，请检查后重试");
    } finally {
      setSubmitting(false);
    }
  };

  const handleClear = () => {
    clearAdminAuth();
    setToken("");
    setError("");
    setHasSavedToken(false);
  };

  return (
    <div className="min-h-screen bg-gray-950 text-white px-4 py-10">
      <div className="max-w-md mx-auto rounded-2xl border border-gray-800 bg-gray-900/80 p-8 shadow-2xl">
        <div className="flex items-center gap-4 mb-8">
          <img src={logo} alt="logo" className="w-14 h-14 rounded-2xl shadow-lg" />
          <div>
            <h1 className="text-2xl font-bold">后台主页访问验证</h1>
            <p className="text-sm text-gray-400 mt-1">
              通过 token 验证后才会显示管理后台主页。分享链接页不受影响。
            </p>
          </div>
        </div>

        <div className="rounded-xl border border-cyan-500/20 bg-cyan-500/10 px-4 py-3 text-sm text-cyan-100 flex items-center gap-3 mb-6">
          <ShieldCheck size={18} />
          未验证前不会加载主页界面和后台数据。
        </div>

        <form className="space-y-5" onSubmit={handleSubmit}>
          <div>
            <label className="block text-sm text-gray-300 mb-2">后台 Token</label>
            <div className="relative">
              <KeyRound size={16} className="absolute left-3 top-3.5 text-gray-500" />
              <input
                type="password"
                value={token}
                onChange={(event) => setToken(event.target.value)}
                className="w-full rounded-xl border border-gray-700 bg-gray-800 pl-10 pr-3 py-3 text-white focus:outline-none focus:border-cyan-500"
                placeholder="请输入服务器 token"
              />
            </div>
          </div>

          {error && (
            <div className="rounded-lg border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm text-red-200">
              {error}
            </div>
          )}

          <div className="flex items-center gap-3">
            <button
              type="submit"
              disabled={submitting}
              className={`flex-1 rounded-xl py-3 font-semibold ${
                submitting
                  ? "bg-gray-700 text-gray-400 cursor-not-allowed"
                  : "bg-cyan-600 hover:bg-cyan-500 text-white"
              }`}
            >
              {submitting ? "验证中..." : "进入后台主页"}
            </button>
            {hasSavedToken && (
              <button
                type="button"
                onClick={handleClear}
                className="rounded-xl border border-gray-700 bg-gray-800 hover:bg-gray-700 px-4 py-3 text-sm text-gray-200"
              >
                清除已保存 token
              </button>
            )}
          </div>
        </form>
      </div>
    </div>
  );
}
