export function normalizeStringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string" && item.length > 0)
    : [];
}

export function normalizeMaybeJsonStringArray(value: unknown): string[] {
  if (Array.isArray(value)) return normalizeStringArray(value);
  if (typeof value !== "string") return [];
  const trimmed = value.trim();
  if (!trimmed) return [];
  try {
    return normalizeStringArray(JSON.parse(trimmed));
  } catch {
    return [trimmed];
  }
}

export function normalizeLookupText(value: string | null | undefined) {
  return (value ?? "").trim().toLowerCase();
}
