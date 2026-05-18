import { DEFAULT_IMPERSONATE_PROMPT } from "../../../contracts/constants/impersonate";

interface BuildImpersonateInstructionArgs {
  customPrompt?: unknown;
  direction?: string | null;
  personaName?: string | null;
  personaDescription?: string | null;
}

function normalizeText(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function punctuateDirection(direction: string): string {
  const trimmed = direction.trim();
  if (!trimmed) return "";

  const lastChar = trimmed[trimmed.length - 1];
  return lastChar && ".!?)]}\"'".includes(lastChar) ? trimmed : `${trimmed}.`;
}

function buildCustomImpersonateInstruction(customPrompt: string, direction: string): string {
  if (!direction) return customPrompt;
  return `${customPrompt} ${punctuateDirection(direction)}`;
}

function renderImpersonateTemplate(
  template: string,
  {
    direction,
    personaName,
    personaDescription,
  }: {
    direction: string;
    personaName: string;
    personaDescription: string;
  },
): string {
  return template
    .split(/\r?\n/)
    .filter((line) => {
      if (!personaDescription && line.includes("{{persona_description}}")) return false;
      if (!direction && line.includes("{{impersonate_direction}}")) return false;
      return true;
    })
    .map((line) =>
      line
        .replaceAll("{{user}}", personaName)
        .replaceAll("{{persona_description}}", personaDescription)
        .replaceAll("{{impersonate_direction}}", direction),
    )
    .join("\n")
    .trim();
}

export function buildImpersonateInstruction({
  customPrompt,
  direction,
  personaName,
  personaDescription,
}: BuildImpersonateInstructionArgs): string {
  const normalizedCustomPrompt = normalizeText(customPrompt);
  const impersonationDirection = normalizeText(direction);
  const personaLabel = normalizeText(personaName) || "{{user}}";
  const description = normalizeText(personaDescription);

  if (normalizedCustomPrompt) {
    const resolvedCustomPrompt = renderImpersonateTemplate(normalizedCustomPrompt, {
      direction: impersonationDirection,
      personaName: personaLabel,
      personaDescription: description,
    });
    return normalizedCustomPrompt.includes("{{impersonate_direction}}")
      ? resolvedCustomPrompt
      : buildCustomImpersonateInstruction(resolvedCustomPrompt, impersonationDirection);
  }

  return renderImpersonateTemplate(DEFAULT_IMPERSONATE_PROMPT, {
    direction: impersonationDirection,
    personaName: personaLabel,
    personaDescription: description,
  });
}
