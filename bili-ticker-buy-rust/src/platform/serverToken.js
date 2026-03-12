export function getPreferredServerToken(storedToken, envToken) {
  const normalizedEnv = typeof envToken === "string" ? envToken.trim() : "";
  if (normalizedEnv) {
    return normalizedEnv;
  }

  const normalizedStored = typeof storedToken === "string" ? storedToken.trim() : "";
  if (normalizedStored) {
    return normalizedStored;
  }

  return "";
}
