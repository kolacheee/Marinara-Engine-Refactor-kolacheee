import type { CharacterData } from "../../contracts/types/character";

type DescriptionExtension = {
  active?: unknown;
  content?: unknown;
};

function parseDescriptionExtensions(value: unknown): DescriptionExtension[] {
  if (Array.isArray(value)) return value as DescriptionExtension[];
  if (typeof value !== "string" || !value.trim()) return [];

  try {
    const parsed = JSON.parse(value);
    return Array.isArray(parsed) ? (parsed as DescriptionExtension[]) : [];
  } catch {
    return [];
  }
}

export function getCharacterDescriptionWithExtensions(data: CharacterData): string {
  const description = typeof data.description === "string" ? data.description : "";
  const rawExtensions = data.extensions?.altDescriptions ?? data.extensions?.descriptionExtensions;
  const activeExtensions = parseDescriptionExtensions(rawExtensions)
    .filter((extension) => extension?.active === true && typeof extension.content === "string")
    .map((extension) => String(extension.content).trim())
    .filter(Boolean);

  return [description, ...activeExtensions].filter(Boolean).join("\n");
}
