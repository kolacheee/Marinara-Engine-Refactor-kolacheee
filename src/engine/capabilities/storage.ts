export type StorageEntity =
  | "agents"
  | "app-settings"
  | "backgrounds"
  | "characters"
  | "character-groups"
  | "character-gallery"
  | "chat-folders"
  | "chat-presets"
  | "chats"
  | "connections"
  | "connection-folders"
  | "custom-tools"
  | "extensions"
  | "gallery"
  | "game-assets"
  | "game-state"
  | "knowledge-sources"
  | "lorebooks"
  | "personas"
  | "persona-groups"
  | "prompt-overrides"
  | "prompts"
  | "regex-scripts"
  | "themes";

export interface StorageListOptions {
  filters?: Record<string, unknown>;
  orderBy?: string;
  descending?: boolean;
  limit?: number;
  before?: string;
}

export interface StorageGateway {
  list<T = unknown>(entity: StorageEntity | string, options?: StorageListOptions): Promise<T[]>;
  get<T = unknown>(entity: StorageEntity | string, id: string): Promise<T | null>;
  create<T = unknown>(entity: StorageEntity | string, value: Record<string, unknown>): Promise<T>;
  update<T = unknown>(entity: StorageEntity | string, id: string, patch: Record<string, unknown>): Promise<T>;
  delete(entity: StorageEntity | string, id: string): Promise<{ deleted: boolean }>;
  request<T = unknown>(method: "GET" | "POST" | "PATCH" | "PUT" | "DELETE", operation: string, payload?: unknown): Promise<T>;
  call<T = unknown>(operation: string, payload?: unknown): Promise<T>;
}
