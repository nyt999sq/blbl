import { isTauriRuntime } from "./runtime";

export async function isPermissionGranted(): Promise<boolean> {
  if (isTauriRuntime()) {
    const api = await import("@tauri-apps/api/notification");
    return api.isPermissionGranted();
  }
  if (typeof Notification === "undefined") {
    return false;
  }
  return Notification.permission === "granted";
}

export async function requestPermission(): Promise<NotificationPermission | "denied"> {
  if (isTauriRuntime()) {
    const api = await import("@tauri-apps/api/notification");
    return api.requestPermission();
  }
  if (typeof Notification === "undefined") {
    return "denied";
  }
  return Notification.requestPermission();
}

export async function sendNotification(input: {
  title: string;
  body?: string;
}): Promise<void> {
  if (isTauriRuntime()) {
    const api = await import("@tauri-apps/api/notification");
    await api.sendNotification(input);
    return;
  }
  if (typeof Notification === "undefined") {
    return;
  }
  if (Notification.permission !== "granted") {
    return;
  }
  try {
    new Notification(input.title, {
      body: input.body,
    });
  } catch (_) {}
}
