function pad2(value) {
  return String(value).padStart(2, "0");
}

export function normalizeOptionalString(value) {
  if (value === null || value === undefined) return null;
  const normalized = String(value).trim();
  return normalized ? normalized : null;
}

export function formatDateToLocalDateTime(date, includeSeconds = true) {
  const year = date.getFullYear();
  const month = pad2(date.getMonth() + 1);
  const day = pad2(date.getDate());
  const hour = pad2(date.getHours());
  const minute = pad2(date.getMinutes());
  const second = pad2(date.getSeconds());
  return includeSeconds
    ? `${year}-${month}-${day} ${hour}:${minute}:${second}`
    : `${year}-${month}-${day} ${hour}:${minute}`;
}

export function normalizeDateTimeLocalValue(value) {
  if (!value && value !== 0) return "";

  if (typeof value === "number") {
    const timestamp = value > 1e12 ? value : value * 1000;
    const date = new Date(timestamp);
    return `${formatDateToLocalDateTime(date, true).replace(" ", "T")}`;
  }

  const raw = String(value).trim();
  if (!raw) return "";

  const normalized = raw.replace(" ", "T");
  const matched = normalized.match(
    /^(\d{4})-(\d{1,2})-(\d{1,2})T(\d{1,2}):(\d{1,2})(?::(\d{1,2}))?(?:\.\d{1,3})?$/
  );

  if (matched) {
    const [, year, month, day, hour, minute, second] = matched;
    const base = `${year}-${pad2(month)}-${pad2(day)}T${pad2(hour)}:${pad2(minute)}`;
    return second ? `${base}:${pad2(second)}` : base;
  }

  const parsed = new Date(raw);
  if (!Number.isNaN(parsed.getTime())) {
    return formatDateToLocalDateTime(parsed, true).replace(" ", "T");
  }

  return normalized;
}
