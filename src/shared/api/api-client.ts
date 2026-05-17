export const ADMIN_SECRET_STORAGE_KEY = "marinara-admin-secret";

export function getAdminSecretHeader(): Record<string, string> {
  const secret = localStorage.getItem(ADMIN_SECRET_STORAGE_KEY);
  return secret ? { "x-admin-secret": secret } : {};
}

export class ApiError extends Error {
  constructor(
    message: string,
    public status = 0,
    public details?: unknown,
  ) {
    super(message);
    this.name = "ApiError";
  }

  get payload() {
    return this.details;
  }
}

async function unavailable(): Promise<never> {
  throw new Error("This server-backed action is deferred until the matching Tauri command slice.");
}

export interface JsonRepairRequest {
  id?: string;
  title?: string;
  endpoint?: string;
  rawJson?: string;
  applyEndpoint: string;
  applyBody?: Record<string, unknown>;
  payload?: unknown;
  error?: string;
  [key: string]: unknown;
}

export function isJsonRepairApiError(_error: unknown): false {
  return false;
}

export function getJsonRepairRequest(_error: unknown): JsonRepairRequest | null {
  return null;
}

export const api = {
  get: <T = unknown>(_path: string, _init?: RequestInit): Promise<T> => unavailable(),
  post: <T = unknown>(_path: string, _body?: unknown, _init?: RequestInit): Promise<T> => unavailable(),
  put: <T = unknown>(_path: string, _body?: unknown, _init?: RequestInit): Promise<T> => unavailable(),
  patch: <T = unknown>(_path: string, _body?: unknown, _init?: RequestInit): Promise<T> => unavailable(),
  delete: <T = unknown>(_path: string, _init?: RequestInit): Promise<T> => unavailable(),
  upload: <T = unknown>(_path: string, _body: FormData, _init?: RequestInit): Promise<T> => unavailable(),
  download: (_path: string, _fallbackFilename?: string): Promise<void> => unavailable(),
  downloadPost: (_path: string, _body: unknown, _fallbackFilename?: string): Promise<void> => unavailable(),
  raw: (_path: string, _init?: RequestInit): Promise<Response> => unavailable(),
};
