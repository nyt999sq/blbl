export async function copyText(
  text,
  {
    navigatorRef = typeof navigator !== "undefined" ? navigator : undefined,
    documentRef = typeof document !== "undefined" ? document : undefined,
    windowRef = typeof window !== "undefined" ? window : undefined,
  } = {}
) {
  if (!text && text !== "") return false;

  try {
    if (navigatorRef?.clipboard?.writeText) {
      await navigatorRef.clipboard.writeText(String(text));
      return true;
    }
  } catch (_) {
    // Fall through to legacy copy strategies.
  }

  try {
    if (documentRef?.createElement && documentRef?.body) {
      const textarea = documentRef.createElement("textarea");
      textarea.value = String(text);
      textarea.setAttribute("readonly", "true");
      textarea.style.position = "fixed";
      textarea.style.top = "-9999px";
      textarea.style.left = "-9999px";
      documentRef.body.appendChild(textarea);
      textarea.focus?.();
      textarea.select?.();
      textarea.setSelectionRange?.(0, textarea.value.length);
      const copied = documentRef.execCommand?.("copy") === true;
      documentRef.body.removeChild(textarea);
      if (copied) {
        return true;
      }
    }
  } catch (_) {
    // Fall through to prompt fallback.
  }

  try {
    if (typeof windowRef?.prompt === "function") {
      windowRef.prompt("当前浏览器不支持自动复制，请手动复制以下内容：", String(text));
      return false;
    }
  } catch (_) {
    // Ignore prompt failures.
  }

  return false;
}
