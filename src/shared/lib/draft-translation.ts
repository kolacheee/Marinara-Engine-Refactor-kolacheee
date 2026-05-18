import { toast } from "sonner";
import { translateText } from "./translate-text";

export async function translateDraftText(text: string): Promise<string | null> {
  const trimmed = text.trim();
  if (!trimmed) return null;

  try {
    return await translateText(trimmed);
  } catch (error) {
    toast.error(error instanceof Error ? error.message : "Failed to translate draft");
    return null;
  }
}
