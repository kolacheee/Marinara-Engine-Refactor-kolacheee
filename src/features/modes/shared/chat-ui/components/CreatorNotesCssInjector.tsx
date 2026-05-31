// ──────────────────────────────────────────────
// Creator Notes CSS Injector
//
// Extracts <style> blocks from each active character's
// creator_notes, sanitises + scopes them, then injects
// a single <style> element into <head>.
//
// Supports three modes:
// - "disabled": no card CSS is injected
// - "exclusive": each character's CSS is scoped to their
//   own messages via [data-card-css="<charId>"]
// - "chat": all card CSS is scoped to .mari-card-css
//   (the entire chat area)
// ──────────────────────────────────────────────
import { useEffect, useMemo } from "react";
import { extractCreatorNotesCss } from "../../../../../shared/lib/creator-notes-css";
import { scopeChatCss } from "../../../../../shared/lib/chat-css";

export type CardCssMode = "disabled" | "exclusive" | "chat";

const CARD_CSS_SCOPE = ".mari-card-css";
const STYLE_ELEMENT_ID = "marinara-card-css";

export function CreatorNotesCssInjector({
  characters,
  chatCharacterIds,
  mode = "chat",
}: {
  characters: Array<{ id: string; data: string; avatarPath: string | null }> | undefined;
  chatCharacterIds: string[];
  mode?: CardCssMode;
}) {
  const scopedCss = useMemo(() => {
    if (mode === "disabled") return "";
    if (!characters?.length || !chatCharacterIds.length) return "";

    const activeIdSet = new Set(chatCharacterIds);
    const cssBlocks: string[] = [];

    for (const char of characters) {
      if (!activeIdSet.has(char.id)) continue;
      try {
        const parsed = typeof char.data === "string" ? JSON.parse(char.data) : char.data;
        const creatorNotes: string = parsed.creator_notes ?? "";
        if (!creatorNotes) continue;
        const { css } = extractCreatorNotesCss(creatorNotes);
        if (!css) continue;

        if (mode === "exclusive") {
          cssBlocks.push(scopeChatCss(css, `${CARD_CSS_SCOPE} [data-card-css="${char.id}"]`));
        } else {
          cssBlocks.push(css);
        }
      } catch {
        // Malformed data — skip silently
      }
    }

    if (!cssBlocks.length) return "";
    return mode === "exclusive" ? cssBlocks.join("\n") : scopeChatCss(cssBlocks.join("\n"), CARD_CSS_SCOPE);
  }, [characters, chatCharacterIds, mode]);

  useEffect(() => {
    let style = document.getElementById(STYLE_ELEMENT_ID) as HTMLStyleElement | null;

    if (!scopedCss) {
      style?.remove();
      return;
    }

    if (!style) {
      style = document.createElement("style");
      style.id = STYLE_ELEMENT_ID;
      document.head.appendChild(style);
    }
    style.textContent = `@layer card-css {\n${scopedCss}\n}`;

    return () => {
      style?.remove();
    };
  }, [scopedCss]);

  return null;
}
