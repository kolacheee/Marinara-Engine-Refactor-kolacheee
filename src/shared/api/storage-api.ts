import type { StorageGateway, StorageListOptions } from "../../engine/capabilities";
import { invokeTauri } from "./tauri-client";

export const storageApi: StorageGateway = {
  list: (entity: string, options?: StorageListOptions) =>
    invokeTauri("api_request", {
      method: "GET",
      path: `/${entity}${optionsToQuery(options)}`,
      body: null,
    }),
  get: (entity: string, id: string) =>
    invokeTauri("api_request", {
      method: "GET",
      path: `/${entity}/${encodeURIComponent(id)}`,
      body: null,
    }),
  create: (entity: string, value: Record<string, unknown>) =>
    invokeTauri("api_request", {
      method: "POST",
      path: `/${entity}`,
      body: value,
    }),
  update: (entity: string, id: string, patch: Record<string, unknown>) =>
    invokeTauri("api_request", {
      method: "PATCH",
      path: `/${entity}/${encodeURIComponent(id)}`,
      body: patch,
    }),
  delete: (entity: string, id: string) =>
    invokeTauri("api_request", {
      method: "DELETE",
      path: `/${entity}/${encodeURIComponent(id)}`,
      body: null,
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

function optionsToQuery(options?: StorageListOptions): string {
  if (!options) return "";
  const params = new URLSearchParams();
  if (options.limit != null) params.set("limit", String(options.limit));
  if (options.before) params.set("before", options.before);
  if (options.orderBy) params.set("orderBy", options.orderBy);
  if (options.descending != null) params.set("descending", String(options.descending));
  if (options.filters) params.set("filters", JSON.stringify(options.filters));
  const query = params.toString();
  return query ? `?${query}` : "";
}
