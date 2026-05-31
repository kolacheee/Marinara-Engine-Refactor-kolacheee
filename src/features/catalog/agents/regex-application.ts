import { useCallback, useMemo } from "react";
import { applyRegexReplacement } from "../../../engine/shared/regex/regex-replacement";
import { useRegexScripts, type RegexScriptRow } from "./hooks/use-regex-scripts";

type RegexPlacement = "ai_output" | "user_input";

export type ScopedRegexMode = "disabled" | "exclusive" | "chat";

interface RegexApplyOptions {
  scopedMode?: ScopedRegexMode;
  characterId?: string;
  depth?: number;
  resolveMacros?: (value: string) => string;
}

interface ParsedRegexScript extends RegexScriptRow {
  enabledBool: boolean;
  promptOnlyBool: boolean;
  placements: RegexPlacement[];
  trimList: string[];
}

function parseJsonArray<T extends string>(value: string, allowed?: Set<T>): T[] {
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((entry): entry is T => typeof entry === "string" && (!allowed || allowed.has(entry as T)));
  } catch {
    return allowed?.has(value as T) ? [value as T] : [];
  }
}

function parseScript(row: RegexScriptRow): ParsedRegexScript {
  return {
    ...row,
    enabledBool: row.enabled === "true" || row.enabled === "1",
    promptOnlyBool: row.promptOnly === "true" || row.promptOnly === "1",
    placements: parseJsonArray(row.placement, new Set<RegexPlacement>(["ai_output", "user_input"])),
    trimList: parseJsonArray(row.trimStrings),
  };
}

function filterForMode(
  scripts: ParsedRegexScript[],
  mode: ScopedRegexMode,
  characterId: string | undefined,
): ParsedRegexScript[] {
  if (mode === "disabled") return scripts.filter((s) => !s.characterId);
  if (mode === "chat") return scripts;
  // "exclusive": global + only the owning character's scripts
  if (!characterId) return scripts.filter((s) => !s.characterId);
  return scripts.filter((s) => !s.characterId || s.characterId === characterId);
}

function resolveText(value: string, options?: RegexApplyOptions): string {
  return options?.resolveMacros ? options.resolveMacros(value) : value;
}

function applyScripts(
  text: string,
  scripts: ParsedRegexScript[],
  placement: RegexPlacement,
  options?: RegexApplyOptions & { promptOnly?: boolean },
): string {
  let result = text;
  for (const script of scripts) {
    if (!script.enabledBool) continue;
    if (!script.placements.includes(placement)) continue;
    if (options?.promptOnly ? !script.promptOnlyBool : script.promptOnlyBool) continue;
    if (options?.depth != null) {
      if (script.minDepth != null && options.depth < script.minDepth) continue;
      if (script.maxDepth != null && options.depth > script.maxDepth) continue;
    }

    try {
      const findRegex = resolveText(script.findRegex, options);
      if (!findRegex) continue;
      const regex = new RegExp(findRegex, script.flags);
      result = applyRegexReplacement(result, regex, script.replaceString, (value) => resolveText(value, options));
      for (const trim of script.trimList) {
        const resolvedTrim = resolveText(trim, options);
        if (resolvedTrim) result = result.split(resolvedTrim).join("");
      }
    } catch {
      // Invalid user regexes are skipped; the editor remains the validation surface.
    }
  }
  return result;
}

/**
 * Hook that provides functions to apply regex transformations.
 *
 * Scoped regex modes control how character-scoped scripts are applied:
 * - `disabled` — scoped scripts are ignored; only global scripts run.
 * - `exclusive` (default) — scoped scripts only apply to their owning character's messages.
 * - `chat` — all scoped scripts apply to every message, including user input.
 */
export function useApplyRegex(characterIds?: string[]) {
  const { data: regexScripts } = useRegexScripts(characterIds);
  const scripts = useMemo(() => (regexScripts ?? []).map(parseScript), [regexScripts]);

  const applyToAIOutput = useCallback(
    (text: string, options?: RegexApplyOptions) => {
      const mode = options?.scopedMode ?? "exclusive";
      const filtered = filterForMode(scripts, mode, options?.characterId);
      return applyScripts(text, filtered, "ai_output", options);
    },
    [scripts],
  );

  const applyToUserInput = useCallback(
    (text: string, options?: RegexApplyOptions) => {
      const mode = options?.scopedMode ?? "exclusive";
      const filtered = filterForMode(scripts, mode, options?.characterId);
      return applyScripts(text, filtered, "user_input", options);
    },
    [scripts],
  );

  const applyPromptOnly = useCallback(
    (text: string, placement: RegexPlacement, options?: RegexApplyOptions) => {
      const mode = options?.scopedMode ?? "exclusive";
      const filtered = filterForMode(scripts, mode, options?.characterId);
      return applyScripts(text, filtered, placement, { ...options, promptOnly: true });
    },
    [scripts],
  );

  return { applyToAIOutput, applyToUserInput, applyPromptOnly };
}
