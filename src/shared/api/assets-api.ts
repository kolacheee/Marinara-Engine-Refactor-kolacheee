import type { AssetGateway } from "../../engine/capabilities/assets";
import { invokeTauri } from "./tauri-client";

export const assetsApi: AssetGateway = {
  list: (path?: string) =>
    invokeTauri("api_request", {
      method: "GET",
      path: path ? `/game-assets?path=${encodeURIComponent(path)}` : "/game-assets",
      body: null,
    }),
  readText: (path: string) =>
    invokeTauri("api_request", {
      method: "GET",
      path: `/game-assets/file-content/${encodeURIComponent(path)}`,
      body: null,
    }).then((value) => String((value as { content?: unknown }).content ?? "")),
  writeText: (path: string, content: string) =>
    invokeTauri("api_request", {
      method: "PUT",
      path: `/game-assets/file-content/${encodeURIComponent(path)}`,
      body: { content },
    }),
  remove: (path: string) =>
    invokeTauri("api_request", {
      method: "DELETE",
      path: `/game-assets/file/${encodeURIComponent(path)}`,
      body: null,
    }),
  copy: (path: string, targetFolder: string) =>
    invokeTauri("api_request", {
      method: "POST",
      path: "/game-assets/copy",
      body: { path, targetFolder },
    }),
  move: (path: string, targetFolder: string) =>
    invokeTauri("api_request", {
      method: "POST",
      path: "/game-assets/move",
      body: { path, targetFolder },
    }),
  openFolder: (path?: string) =>
    invokeTauri("api_request", {
      method: "POST",
      path: "/game-assets/open-folder",
      body: { subfolder: path },
    }),
};
