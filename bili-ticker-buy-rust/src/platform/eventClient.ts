import { ensureWebSession } from "./apiClient";
import { getWsBase, isTauriRuntime } from "./runtime";

type EventCallback = (event: { payload: any }) => void;

const listeners = new Map<string, Set<EventCallback>>();
let ws: WebSocket | null = null;
let wsConnecting: Promise<void> | null = null;

function dispatch(type: string, payload: any) {
  const callbacks = listeners.get(type);
  if (!callbacks) return;
  for (const cb of callbacks) {
    cb({ payload });
  }
}

async function connectWebSocket() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
    return;
  }
  if (wsConnecting) return wsConnecting;

  wsConnecting = (async () => {
    const session = await ensureWebSession();
    if (!session) {
      throw new Error("missing web session for websocket");
    }
    const url = `${getWsBase()}/api/ws?session=${encodeURIComponent(session)}`;
    ws = new WebSocket(url);
    ws.onmessage = (message) => {
      try {
        const data = JSON.parse(message.data);
        if (data?.type) {
          dispatch(data.type, data);
        }
      } catch (_) {}
    };
    ws.onclose = () => {
      ws = null;
      setTimeout(() => {
        if (listeners.size > 0) {
          connectWebSocket().catch(() => {});
        }
      }, 1000);
    };
    await new Promise<void>((resolve, reject) => {
      if (!ws) return reject(new Error("websocket not created"));
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error("websocket open failed"));
    });
  })().finally(() => {
    wsConnecting = null;
  });

  return wsConnecting;
}

export async function listen(event: string, callback: EventCallback): Promise<() => void> {
  if (isTauriRuntime()) {
    const eventApi = await import("@tauri-apps/api/event");
    return eventApi.listen(event, callback);
  }

  if (!listeners.has(event)) {
    listeners.set(event, new Set());
  }
  listeners.get(event)!.add(callback);
  await connectWebSocket();

  return () => {
    const callbacks = listeners.get(event);
    if (!callbacks) return;
    callbacks.delete(callback);
    if (callbacks.size === 0) {
      listeners.delete(event);
    }
  };
}
