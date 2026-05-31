// ──────────────────────────────────────────────
// Regex Script Import: ST card extraction
// ──────────────────────────────────────────────
import type { RegexPlacement } from "../../engine/contracts/types/regex";

interface STRegexScriptRaw {
  findRegex?: string;
  scriptName?: string;
  name?: string;
  replaceString?: string;
  trimStrings?: unknown;
  placement?: unknown;
  disabled?: unknown;
  enabled?: unknown;
  promptOnly?: unknown;
  minDepth?: unknown;
  maxDepth?: unknown;
}

export interface ExtractedRegexScript {
  characterId: string;
  name: string;
  enabled: boolean;
  findRegex: string;
  replaceString: string;
  trimStrings: string[];
  placement: string;
  flags: string;
  promptOnly: boolean;
  order: number;
  minDepth: number | null;
  maxDepth: number | null;
}

function convertSTPlacement(placement: unknown): RegexPlacement[] {
  if (Array.isArray(placement)) {
    return placement.flatMap((p) => convertSTPlacement(p));
  }
  if (typeof placement === "number") {
    if (placement === 1) return ["user_input"];
    if (placement === 2) return ["ai_output"];
    return ["ai_output"];
  }
  if (typeof placement === "string") {
    if (placement === "user_input") return ["user_input"];
    if (placement === "ai_output") return ["ai_output"];
  }
  return ["ai_output"];
}

function parseSTRegexPattern(findRegex: string): { pattern: string; flags: string } {
  const match = findRegex.match(/^\/(.+)\/([gimsuy]*)$/s);
  if (match) return { pattern: match[1] ?? findRegex, flags: match[2] ?? "gi" };
  return { pattern: findRegex, flags: "gi" };
}

function convertSTRegexScript(
  raw: STRegexScriptRaw,
  characterId: string,
  order: number,
): ExtractedRegexScript | null {
  const findRegex = typeof raw.findRegex === "string" ? raw.findRegex : null;
  if (!findRegex) return null;

  const { pattern, flags } = parseSTRegexPattern(findRegex);
  if (!pattern) return null;

  const name =
    typeof raw.scriptName === "string" && raw.scriptName.trim()
      ? raw.scriptName.trim()
      : typeof raw.name === "string" && raw.name.trim()
        ? raw.name.trim()
        : `Script ${order + 1}`;

  const enabled = raw.disabled != null ? !raw.disabled : raw.enabled != null ? Boolean(raw.enabled) : true;
  const trimStrings: string[] = Array.isArray(raw.trimStrings) ? raw.trimStrings.map(String) : [];

  return {
    characterId,
    name,
    enabled,
    findRegex: pattern,
    replaceString: typeof raw.replaceString === "string" ? raw.replaceString : "",
    trimStrings,
    placement: JSON.stringify(convertSTPlacement(raw.placement)),
    flags,
    promptOnly: Boolean(raw.promptOnly),
    order,
    minDepth: typeof raw.minDepth === "number" ? raw.minDepth : null,
    maxDepth: typeof raw.maxDepth === "number" ? raw.maxDepth : null,
  };
}

/**
 * Extract embedded regex scripts from a parsed SillyTavern character card payload.
 * Returns an array of regex script records ready to be persisted via storageApi.create.
 */
export function extractEmbeddedRegexScripts(
  characterData: Record<string, unknown>,
  characterId: string,
): ExtractedRegexScript[] {
  const extensions = characterData.extensions as Record<string, unknown> | undefined;
  if (!extensions) return [];
  const scripts = extensions.regex_scripts;
  if (!Array.isArray(scripts)) return [];

  const results: ExtractedRegexScript[] = [];
  for (let i = 0; i < scripts.length; i++) {
    const raw = scripts[i];
    if (!raw || typeof raw !== "object") continue;
    const converted = convertSTRegexScript(raw as STRegexScriptRaw, characterId, i);
    if (converted) results.push(converted);
  }
  return results;
}
