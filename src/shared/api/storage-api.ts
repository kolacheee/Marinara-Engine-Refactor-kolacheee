import type { StorageGateway, StorageListOptions } from "../../engine/capabilities/storage";
import { invokeTauri } from "./tauri-client";

export const storageApi: StorageGateway = {
  list: (entity: string, options?: StorageListOptions) =>
    invokeTauri("storage_list", {
      entity,
      options: options ?? null,
    }),
  get: (entity: string, id: string) =>
    invokeTauri("storage_get", {
      entity,
      id,
    }),
  create: (entity: string, value: Record<string, unknown>) =>
    invokeTauri("storage_create", {
      entity,
      value,
    }),
  update: (entity: string, id: string, patch: Record<string, unknown>) =>
    invokeTauri("storage_update", {
      entity,
      id,
      patch,
    }),
  delete: (entity: string, id: string) =>
    invokeTauri("storage_delete", {
      entity,
      id,
    }),
  request: (method, operation, payload?: unknown) =>
    invokeTauri("api_request", {
      method,
      path: operation.startsWith("/") ? operation : `/${operation}`,
      body: payload ?? null,
    }),
  call: (operation: string, payload?: unknown) =>
    invokeTauri("api_request", {
      method: "POST",
      path: operation.startsWith("/") ? operation : `/${operation}`,
      body: payload ?? null,
    }),
};
