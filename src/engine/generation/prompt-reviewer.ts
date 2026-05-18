import type { LlmGateway, StorageGateway } from "../capabilities";

export type PromptReviewInput = {
  presetId: string;
  connectionId: string;
  focusAreas?: string[];
};

export type PromptReviewEvent =
  | { type: "token"; data: string }
  | { type: "done"; data: string }
  | { type: "error"; data: string };

const PROMPT_REVIEWER_SYSTEM_PROMPT = `You are an expert prompt engineer reviewing prompt presets for AI roleplay applications. Analyze the prompt structure and content, then return a structured review in JSON:

{
  "overall_score": 8,
  "summary": "Brief 1-2 sentence overall assessment",
  "sections": [
    {
      "area": "clarity",
      "score": 8,
      "findings": "What you found",
      "suggestions": ["Specific improvement 1", "Specific improvement 2"]
    }
  ],
  "token_estimate": 2500,
  "warnings": ["Any critical issues"],
  "best_practices": ["Things done well"]
}

Review areas:
- clarity: Are instructions clear and unambiguous?
- consistency: Are there contradictory instructions?
- coverage: Are all important aspects covered?
- jailbreak_safety: Are there safeguards and obvious bypass risks?
- token_efficiency: Is the prompt concise and context-efficient?
- role_balance: Are system/user/assistant roles used appropriately?

Be specific and actionable. Reference exact sections when possible.`;

type JsonRecord = Record<string, unknown>;

export async function* reviewPromptPreset(
  capabilities: { storage: StorageGateway; llm: LlmGateway },
  input: PromptReviewInput,
  signal?: AbortSignal,
): AsyncGenerator<PromptReviewEvent> {
  if (signal?.aborted) throw new DOMException("The operation was aborted.", "AbortError");
  const preset = await capabilities.storage.get<JsonRecord>("prompts", input.presetId);
  if (!preset) throw new Error("Prompt preset not found.");

  const focusAreas = input.focusAreas?.length ? input.focusAreas : ["clarity", "consistency", "coverage"];
  const assembledView = await assemblePromptReviewView(capabilities.storage, input.presetId);
  const userPrompt = [
    `Review this prompt preset. Focus areas: ${focusAreas.join(", ")}`,
    "",
    `Preset Name: ${stringValue(preset.name) || "Prompt preset"}`,
    `Wrap Format: ${stringValue(preset.wrapFormat) || "xml"}`,
    `Description: ${stringValue(preset.description) || "(none)"}`,
    "",
    `Assembled Prompt (${assembledView.length} characters):`,
    "",
    assembledView,
  ].join("\n");

  const raw = await capabilities.llm.complete(
    {
      connectionId: input.connectionId,
      messages: [
        { role: "system", content: PROMPT_REVIEWER_SYSTEM_PROMPT },
        { role: "user", content: userPrompt },
      ],
      parameters: { temperature: 0.7, maxTokens: 8192 },
    },
    signal,
  );
  yield { type: "token", data: raw };
  yield { type: "done", data: raw };
}

async function assemblePromptReviewView(storage: StorageGateway, presetId: string): Promise<string> {
  const sections = await storage.list<JsonRecord>("prompt-sections", { filters: { presetId } });
  const enabledSections = sections
    .filter((section) => section.enabled !== false)
    .sort((a, b) => orderValue(a) - orderValue(b));

  if (enabledSections.length === 0) {
    return "(Preset has no enabled sections.)";
  }

  return enabledSections
    .map((section, index) => {
      const name = stringValue(section.name) || stringValue(section.identifier) || "Untitled Section";
      const role = (stringValue(section.role) || "system").toUpperCase();
      const content = stringValue(section.content);
      return `[Message ${index + 1} | ${role} | ${name}]\n${content.trim() ? content : "(empty)"}`;
    })
    .join("\n\n---\n\n");
}

function orderValue(section: JsonRecord): number {
  const value = section.sortOrder ?? section.order ?? section.injectionOrder;
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}
