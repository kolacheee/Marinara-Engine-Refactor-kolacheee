import { api } from "./api-client";

export interface ImportFilePayload {
  file: File;
  fields?: Record<string, string | number | boolean | null | undefined>;
}

export async function importJson<T>(path: string, payload: unknown): Promise<T> {
  return api.post<T>(path, payload);
}

export async function importFile<T>(path: string, payload: ImportFilePayload | File): Promise<T> {
  const file = payload instanceof File ? payload : payload.file;
  const fields = payload instanceof File ? undefined : payload.fields;
  const formData = new FormData();
  for (const [key, value] of Object.entries(fields ?? {})) {
    if (value !== null && value !== undefined) formData.append(key, String(value));
  }
  formData.append("file", file, file.name);
  return api.upload<T>(path, formData);
}

export const importApi = {
  marinara: <T>(envelope: unknown) => importJson<T>("/import/marinara", envelope),
  marinaraFile: <T>(file: File) => importFile<T>("/import/marinara-file", file),
  stCharacterJson: <T>(payload: unknown) => importJson<T>("/import/st-character", payload),
  stCharacterFile: <T>(payload: ImportFilePayload) => importFile<T>("/import/st-character", payload),
  stChat: <T>(file: File) => importFile<T>("/import/st-chat", file),
  stChatIntoGroup: <T>(chatId: string, file: File) =>
    importFile<T>("/import/st-chat-into-group", { file, fields: { chatId } }),
  stPreset: <T>(payload: unknown) => importJson<T>("/import/st-preset", payload),
  stLorebook: <T>(payload: unknown) => importJson<T>("/import/st-lorebook", payload),
  stBulkScan: <T>(payload: unknown) => importJson<T>("/import/st-bulk/scan", payload),
  stBulkRun: <T>(payload: unknown) => importJson<T>("/import/st-bulk/run", payload),
  stBulkRunEvents: (payload: unknown, signal?: AbortSignal) => api.streamEvents("/import/st-bulk/run", payload, signal),
  listDirectory: <T>(path: string) => importJson<T>("/import/list-directory", { path }),
};
