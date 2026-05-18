// ──────────────────────────────────────────────
// Hooks: Installed Extensions
// ──────────────────────────────────────────────
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../../shared/api/api-client";
import type { CreateExtensionInput, UpdateExtensionInput } from "../../../engine/contracts/schemas/extension.schema";
import type { InstalledExtension } from "../../../engine/contracts/types/extension";

export const extensionKeys = {
  all: ["extensions"] as const,
  list: () => [...extensionKeys.all, "list"] as const,
};

export function useExtensions() {
  return useQuery({
    queryKey: extensionKeys.list(),
    queryFn: () => api.get<InstalledExtension[]>("/extensions"),
    staleTime: 0,
    refetchOnWindowFocus: true,
    refetchOnReconnect: true,
    refetchInterval: () => (document.hidden ? false : 15_000),
  });
}

export function useCreateExtension() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (data: CreateExtensionInput) => api.post<InstalledExtension>("/extensions", data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: extensionKeys.all });
    },
  });
}

export function useUpdateExtension() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...data }: { id: string } & UpdateExtensionInput) =>
      api.patch<InstalledExtension>(`/extensions/${id}`, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: extensionKeys.all });
    },
  });
}

export function useDeleteExtension() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.delete(`/extensions/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: extensionKeys.all });
    },
  });
}
