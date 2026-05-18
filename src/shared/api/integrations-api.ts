import type { IntegrationGateway } from "../../engine/capabilities/integrations";
import { invokeTauri } from "./tauri-client";

export const integrationsApi: IntegrationGateway = {
  call: (integration: string, operation: string, payload?: unknown) =>
    invokeTauri("api_request", {
      method: "POST",
      path: `/${integration}/${operation}`,
      body: payload ?? null,
    }),
};
