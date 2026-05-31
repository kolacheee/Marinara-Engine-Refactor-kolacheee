// ──────────────────────────────────────────────
// Chat CSS: sanitisation & scoping utilities
// ──────────────────────────────────────────────

const CHAT_STYLE_BLOCK_RE = /<style\b[^>]*>([\s\S]*?)<\/style>/gi;
const CSS_SELECTOR_RE = /(^|[{}])\s*([^@{}][^{]*)\{/g;

// ── Theme-token blocklist ────────────────────
// App theme custom properties (shadcn / Tailwind) that card CSS must NOT
// override. Declarations like `--background: red;` are stripped so a card
// cannot repaint the application UI outside the chat surface.
const BLOCKED_THEME_TOKENS: readonly string[] = [
  "background",
  "foreground",
  "card",
  "card-foreground",
  "popover",
  "popover-foreground",
  "primary",
  "primary-foreground",
  "secondary",
  "secondary-foreground",
  "muted",
  "muted-foreground",
  "accent",
  "accent-foreground",
  "destructive",
  "destructive-foreground",
  "border",
  "input",
  "ring",
  "radius",
  "sidebar",
  "sidebar-foreground",
  "sidebar-border",
  "sidebar-accent",
  "sidebar-accent-foreground",
  "color-background",
  "color-foreground",
  "color-card",
  "color-card-foreground",
  "color-popover",
  "color-popover-foreground",
  "color-primary",
  "color-primary-foreground",
  "color-secondary",
  "color-secondary-foreground",
  "color-muted",
  "color-muted-foreground",
  "color-accent",
  "color-accent-foreground",
  "color-destructive",
  "color-destructive-foreground",
  "color-border",
  "color-input",
  "color-ring",
  "color-sidebar",
  "color-sidebar-foreground",
  "color-sidebar-border",
  "color-sidebar-accent",
  "color-sidebar-accent-foreground",
] as const;

const sortedTokens = [...BLOCKED_THEME_TOKENS].sort((a, b) => b.length - a.length);

const THEME_TOKEN_RE = new RegExp(
  `--(?:${sortedTokens.join("|")})\\s*:[^;}]+;?`,
  "g",
);

/** Extract `<style>` blocks from HTML and return the CSS + the HTML without them. */
export function extractChatStyleBlocks(html: string): { html: string; css: string } {
  const cssBlocks: string[] = [];
  const withoutStyles = html.replace(CHAT_STYLE_BLOCK_RE, (_match, css: string) => {
    cssBlocks.push(css);
    return "";
  });
  return { html: withoutStyles, css: cssBlocks.join("\n") };
}

/**
 * Strip known-dangerous CSS constructs while preserving visual features
 * like animations, gradients, pseudo-elements, and safe `url()` values.
 *
 * Also strips declarations of app theme custom properties to prevent card
 * CSS from repainting the application UI outside the chat surface.
 */
export function sanitizeChatCss(css: string): string {
  return (
    css
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/<\/?style\b[^>]*>/gi, "")
      .replace(/@import\s+[^;]+;?/gi, "")
      .replace(/@namespace\s+[^;]+;?/gi, "")
      .replace(/expression\s*\([^)]*\)/gi, "")
      .replace(/javascript\s*:/gi, "")
      .replace(/vbscript\s*:/gi, "")
      .replace(/behavior\s*:/gi, "x-behavior:")
      .replace(/-moz-binding\s*:/gi, "x-moz-binding:")
      .replace(/url\s*\(\s*(['"]?)(?!data:image\/|https?:\/\/)[^)]+\)/gi, "none")
      .replace(/position\s*:\s*fixed/gi, "position: absolute")
      .replace(/<\/style/gi, "<\\/style")
      .replace(THEME_TOKEN_RE, "/* [blocked] */")
      .replace(/!important/gi, "")
      .trim()
  );
}

/**
 * Namespace `@keyframes` names with a prefix so card animations cannot
 * collide with application-level animations.
 */
function namespaceKeyframes(css: string, prefix: string): string {
  const names: string[] = [];

  let result = css.replace(/@keyframes\s+([\w-]+)/g, (_m, name: string) => {
    names.push(name);
    return `@keyframes ${prefix}${name}`;
  });

  if (!names.length) return result;

  names.sort((a, b) => b.length - a.length);
  const escaped = names.map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const nameRe = new RegExp(`(?<![\\w-])(${escaped.join("|")})(?![\\w-])`, "g");

  result = result.replace(
    /(animation(?:-name)?\s*:[^;{}]*)/g,
    (_m, propDecl: string) => propDecl.replace(nameRe, `${prefix}$1`),
  );

  return result;
}

const ROOT_SELECTOR_RE = /^(:root|html|body)(?=$|[\s[:.#>+~,])/;

/**
 * Prefix every CSS selector with a scope class so the styles only apply
 * inside a designated container. Keyframe selectors (`from`, `to`, `%`)
 * are preserved as-is; `body`/`html`/`:root` are rewritten to the scope.
 *
 * Also namespaces `@keyframes` to prevent global animation-name collisions
 * and strips app theme custom-property overrides.
 */
export function scopeChatCss(css: string, scopeSelector: string): string {
  const sanitized = sanitizeChatCss(css);
  if (!sanitized) return "";

  const withNsKeyframes = namespaceKeyframes(sanitized, "mc-");

  return withNsKeyframes.replace(CSS_SELECTOR_RE, (_match, boundary: string, selectors: string) => {
    const scopedSelectors = selectors
      .split(",")
      .map((selector) => {
        const trimmed = selector.trim();
        if (!trimmed) return "";
        if (/^(from|to|\d+(?:\.\d+)?%)$/i.test(trimmed)) return trimmed;
        if (trimmed.startsWith(scopeSelector)) return trimmed;
        if (ROOT_SELECTOR_RE.test(trimmed)) {
          return trimmed.replace(ROOT_SELECTOR_RE, scopeSelector);
        }
        return `${scopeSelector} ${trimmed}`;
      })
      .filter(Boolean)
      .join(", ");
    return `${boundary} ${scopedSelectors}{`;
  });
}
