import { getApiBase, isTauriRuntime } from "./runtime";
import { getPreferredServerToken } from "./serverToken";

type InvokeArgs = Record<string, any> | undefined;

type ApiClientError = Error & {
  data?: any;
  statusCode?: number;
};

let sessionPromise: Promise<string | null> | null = null;
const ADMIN_AUTH_INVALID_EVENT = "bili-admin-auth-invalid";

function loadStoredSession(): string | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage.getItem("bili_headless_session");
}

function saveStoredSession(session: string): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem("bili_headless_session", session);
}

function clearStoredSession(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem("bili_headless_session");
}

function clearStoredServerToken(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem("bili_headless_server_token");
}

function dispatchAdminAuthInvalid(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(ADMIN_AUTH_INVALID_EVENT));
}

async function requestWebSession(token: string): Promise<string | null> {
  const apiBase = getApiBase();
  const headers: Record<string, string> = {};
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }
  const response = await fetch(`${apiBase}/api/auth/token-login`, {
    method: "POST",
    headers,
  });
  if (!response.ok) {
    const error = new Error(`token-login failed: ${response.status}`) as ApiClientError;
    error.statusCode = response.status;
    throw error;
  }
  const data = await response.json();
  if (data?.session) {
    saveStoredSession(data.session);
    return data.session as string;
  }
  return null;
}

function loadServerToken(): string {
  const fromEnv = import.meta.env.VITE_HEADLESS_SERVER_TOKEN;
  if (typeof localStorage === "undefined") {
    return getPreferredServerToken("", fromEnv);
  }

  const existing = localStorage.getItem("bili_headless_server_token");
  const preferred = getPreferredServerToken(existing, fromEnv);
  if (preferred) {
    if (preferred !== existing) {
      localStorage.setItem("bili_headless_server_token", preferred);
    }
    return preferred;
  }

  return "";
}

export async function ensureWebSession(): Promise<string | null> {
  const existing = loadStoredSession();
  if (existing) return existing;

  if (sessionPromise) return sessionPromise;
  sessionPromise = (async () => {
    const token = loadServerToken();
    try {
      return await requestWebSession(token);
    } catch (error) {
      const envToken = getPreferredServerToken("", import.meta.env.VITE_HEADLESS_SERVER_TOKEN);
      const shouldRetryWithFreshToken =
        (error as ApiClientError)?.statusCode === 401 &&
        typeof localStorage !== "undefined" &&
        localStorage.getItem("bili_headless_server_token") !== envToken;

      if (shouldRetryWithFreshToken) {
        clearStoredSession();
        clearStoredServerToken();
        return requestWebSession(loadServerToken());
      }
      clearStoredSession();
      clearStoredServerToken();
      dispatchAdminAuthInvalid();
      throw error;
    }
  })().finally(() => {
    sessionPromise = null;
  });

  return sessionPromise;
}

async function webRequest(
  method: string,
  path: string,
  body?: unknown,
  requireSession = true
): Promise<any> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };

  if (requireSession) {
    const session = await ensureWebSession();
    if (!session) {
      throw new Error("missing session");
    }
    headers["x-session-token"] = session;
  }

  const response = await fetch(`${getApiBase()}${path}`, {
    method,
    headers,
    body: body == null ? undefined : JSON.stringify(body),
  });

  if (response.status === 401 && requireSession) {
    clearStoredSession();
  }

  if (!response.ok) {
    let message = `${response.status}`;
    let data: any = null;
    try {
      data = await response.json();
      message = data?.error || message;
    } catch (_) {}
    const error = new Error(message) as ApiClientError;
    error.data = data;
    error.statusCode = response.status;
    throw error;
  }

  const contentType = response.headers.get("content-type") || "";
  if (!contentType.includes("application/json")) {
    return null;
  }
  return response.json();
}

async function publicRequest(method: string, path: string, body?: unknown): Promise<any> {
  return webRequest(method, path, body, false);
}

async function invokeWeb(command: string, args: InvokeArgs): Promise<any> {
  switch (command) {
    case "get_accounts":
      return webRequest("GET", "/api/accounts");
    case "add_account":
      return webRequest("POST", "/api/accounts/import-cookie", {
        cookies: args?.cookies || [],
      });
    case "remove_account":
      return webRequest("DELETE", `/api/accounts/${args?.uid}`);
    case "get_history":
      return webRequest("GET", "/api/history");
    case "clear_history":
      return webRequest("DELETE", "/api/history");
    case "get_project_history":
      return webRequest("GET", "/api/project-history");
    case "add_project_history":
      return webRequest("POST", "/api/project-history", {
        item: args?.item,
      });
    case "remove_project_history":
      return webRequest(
        "DELETE",
        `/api/project-history?project_id=${encodeURIComponent(
          args?.projectId || args?.project_id || ""
        )}&sku_id=${encodeURIComponent(args?.skuId || args?.sku_id || "")}`
      );
    case "get_user_info":
      return webRequest("POST", "/api/user/info", {
        cookies: args?.cookies || [],
      });
    case "get_login_qrcode": {
      const data = await webRequest("GET", "/api/login/qrcode", undefined, false);
      return [data?.url || "", data?.qrcode_key || ""];
    }
    case "poll_login_status": {
      const key = args?.qrcodeKey || args?.qrcode_key || "";
      const data = await webRequest(
        "GET",
        `/api/login/poll?qrcode_key=${encodeURIComponent(key)}`,
        undefined,
        false
      );
      if (data?.status === "success" && Array.isArray(data.cookies)) {
        return JSON.stringify(data.cookies);
      }
      return data?.message || "登录未完成";
    }
    case "fetch_project":
      return webRequest("POST", "/api/project/fetch", { id: args?.id || "" });
    case "fetch_buyer_list":
      return webRequest("POST", "/api/project/buyers", {
        project_id: args?.projectId || args?.project_id || "",
        cookies: args?.cookies || [],
      });
    case "fetch_address_list":
      return webRequest("POST", "/api/project/addresses", {
        cookies: args?.cookies || [],
      });
    case "sync_time":
      return webRequest("POST", "/api/time/sync", {
        server_url: args?.serverUrl || args?.server_url || null,
      });
    case "start_buy":
      return webRequest("POST", "/api/task/start", {
        ticketInfo: args?.ticketInfo || args?.ticket_info || "",
        interval: args?.interval || 1000,
        mode: args?.mode || 0,
        totalAttempts: args?.totalAttempts || args?.total_attempts || 1,
        timeStart: args?.timeStart || args?.time_start || null,
        proxy: args?.proxy || null,
        timeOffset: args?.timeOffset || args?.time_offset || null,
        buyers: args?.buyers || null,
        ntpServer: args?.ntpServer || args?.ntp_server || null,
      }).then((data) => data?.task_id || "");
    case "stop_task":
      return webRequest("POST", "/api/task/stop", {
        taskId: args?.taskId || args?.task_id || "",
      });
    case "open_bilibili_home":
      if (typeof window !== "undefined") {
        window.open("https://www.bilibili.com", "_blank", "noopener,noreferrer");
      }
      return null;
    default:
      throw new Error(`unsupported command in web mode: ${command}`);
  }
}

export async function invoke(command: string, args?: InvokeArgs): Promise<any> {
  if (isTauriRuntime()) {
    const tauri = await import("@tauri-apps/api/tauri");
    return tauri.invoke(command, args);
  }
  return invokeWeb(command, args);
}

export async function createSharePreset(payload: Record<string, any>): Promise<any> {
  if (isTauriRuntime()) {
    throw new Error("分享链接仅支持 headless Web 模式");
  }
  return webRequest("POST", "/api/share/presets", payload);
}

export async function listSharePresets(): Promise<any> {
  if (isTauriRuntime()) {
    return [];
  }
  return webRequest("GET", "/api/share/presets");
}

export async function closeSharePreset(id: string): Promise<any> {
  if (isTauriRuntime()) {
    throw new Error("分享链接仅支持 headless Web 模式");
  }
  return webRequest("POST", `/api/share/presets/${encodeURIComponent(id)}/close`);
}

export async function getPublicSharePreset(token: string): Promise<any> {
  return publicRequest("GET", `/api/share/${encodeURIComponent(token)}`);
}

export async function fetchShareBuyers(token: string, cookies: string[]): Promise<any> {
  return publicRequest("POST", `/api/share/${encodeURIComponent(token)}/buyers`, {
    cookies,
  });
}

export async function fetchShareAddresses(token: string, cookies: string[]): Promise<any> {
  return publicRequest("POST", `/api/share/${encodeURIComponent(token)}/addresses`, {
    cookies,
  });
}

export async function submitSharePreset(
  token: string,
  payload: Record<string, any>
): Promise<any> {
  return publicRequest("POST", `/api/share/${encodeURIComponent(token)}/submit`, payload);
}

export async function batchDeleteSharePresets(ids: string[]): Promise<any> {
  if (isTauriRuntime()) {
    throw new Error("批量删除分享链接仅支持 headless Web 模式");
  }
  return webRequest("POST", "/api/share/presets/batch-delete", { ids });
}

export async function exportSharePresetConfig(id: string): Promise<any> {
  if (isTauriRuntime()) {
    throw new Error("导出代抢配置仅支持 headless Web 模式");
  }
  return webRequest("GET", `/api/share/presets/${encodeURIComponent(id)}/export-config`);
}

export async function loginWithAdminToken(token: string): Promise<string | null> {
  const normalized = token.trim();
  if (typeof localStorage !== "undefined") {
    localStorage.setItem("bili_headless_server_token", normalized);
  }
  try {
    const session = await requestWebSession(normalized);
    return session;
  } catch (error) {
    clearStoredSession();
    clearStoredServerToken();
    throw error;
  }
}

export function clearAdminAuth(): void {
  clearStoredSession();
  clearStoredServerToken();
}

export function getStoredAdminToken(): string {
  return loadServerToken();
}

export function onAdminAuthInvalid(handler: () => void): () => void {
  if (typeof window === "undefined") {
    return () => {};
  }
  window.addEventListener(ADMIN_AUTH_INVALID_EVENT, handler);
  return () => window.removeEventListener(ADMIN_AUTH_INVALID_EVENT, handler);
}
