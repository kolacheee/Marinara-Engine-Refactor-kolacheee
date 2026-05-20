import { invokeTauri } from "./tauri-client";
import { downloadPayloadFromApiValue, type DownloadPayload } from "./download-payload";

export async function exportProfile(): Promise<DownloadPayload> {
  const value = await invokeTauri("profile_export");
  return downloadPayloadFromApiValue(value, "marinara-profile.json", "application/json");
}

export async function importProfile<T>(envelope: unknown): Promise<T> {
  return invokeTauri<T>("profile_import", { envelope });
}

export async function importProfileFile<T>(path: string): Promise<T> {
  return invokeTauri<T>("profile_import_file", { path });
}

export const profileApi = {
  exportProfile,
  importProfile,
  importProfileFile,
};
