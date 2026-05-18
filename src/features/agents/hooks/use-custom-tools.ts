// ──────────────────────────────────────────────
// Hooks: Custom Tools (React Query)
// ──────────────────────────────────────────────
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../../../shared/api/api-client";

export interface CustomToolRow {
  id: string;
  name: string;
  description: string;
  parametersSchema: string;
  executionType: string;
  webhookUrl: string | null;
  staticResult: string | null;
  enabled: string;
  createdAt: string;
  updatedAt: string;
}

export interface CustomToolCapabilities {
  staticResults?: boolean;
  webhooks?: boolean;
  scriptExecutionEnabled?: boolean;
}

export function isCustomToolSelectable(tool: CustomToolRow, _capabilities?: CustomToolCapabilities | null): boolean {
  const enabled = tool.enabled === "true" || tool.enabled === "1";
  if (!enabled) return false;
  if (tool.executionType === "static") return !!tool.staticResult?.trim();
  if (tool.executionType === "webhook") return !!tool.webhookUrl?.trim();
  return false;
}

const toolKeys = {
  all: ["custom-tools"] as const,
  detail: (id: string) => ["custom-tools", id] as const,
  capabilities: ["custom-tools", "capabilities"] as const,
};

export function useCustomTools() {
  return useQuery({
    queryKey: toolKeys.all,
    queryFn: () => api.get<CustomToolRow[]>("/custom-tools"),
  });
}

export function useCustomTool(id: string | null) {
  return useQuery({
    queryKey: toolKeys.detail(id ?? ""),
    queryFn: () => api.get<CustomToolRow>(`/custom-tools/${id}`),
    enabled: !!id,
  });
}

export function useCustomToolCapabilities() {
  return useQuery({
    queryKey: toolKeys.capabilities,
    queryFn: () => api.get<CustomToolCapabilities>("/custom-tools/capabilities"),
  });
}

export function useCreateCustomTool() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (data: Record<string, unknown>) => api.post("/custom-tools", data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: toolKeys.all });
    },
  });
}

export function useUpdateCustomTool() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...data }: { id: string } & Record<string, unknown>) => api.patch(`/custom-tools/${id}`, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: toolKeys.all });
    },
  });
}

export function useDeleteCustomTool() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.delete(`/custom-tools/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: toolKeys.all });
    },
  });
}
