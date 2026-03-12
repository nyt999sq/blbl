declare global {
  interface Window {
    __TAURI__?: unknown;
    __TAURI_IPC__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  }
}

export function isTauriRuntime(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  return Boolean(
    window.__TAURI__ || window.__TAURI_IPC__ || window.__TAURI_INTERNALS__
  );
}

export function getApiBase(): string {
  const base = import.meta.env.VITE_API_BASE;
  if (typeof base === "string") {
    return base.replace(/\/$/, "");
  }
  return "";
}

export function getWsBase(): string {
  const explicit = import.meta.env.VITE_WS_BASE;
  if (typeof explicit === "string" && explicit) {
    return explicit.replace(/\/$/, "");
  }

  if (typeof window === "undefined") {
    return "ws://127.0.0.1:18080";
  }
  const protocol = window.location.protocol === "https:" ? "wss" : "ws";
  return `${protocol}://${window.location.host}`;
}
