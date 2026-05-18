// ──────────────────────────────────────────────
// Hooks: Custom Themes
// ──────────────────────────────────────────────
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../../shared/api/api-client";
import type { CreateThemeInput, UpdateThemeInput } from "../../../engine/contracts/schemas/theme.schema";
import type { Theme } from "../../../engine/contracts/types/theme";

export const themeKeys = {
  all: ["themes"] as const,
  list: () => [...themeKeys.all, "list"] as const,
};

export function findDuplicateTheme(themes: Theme[], name: string, css: string) {
  return themes.find((theme) => theme.name === name && theme.css === css) ?? null;
}

export function useThemes() {
  return useQuery({
    queryKey: themeKeys.list(),
    queryFn: () => api.get<Theme[]>("/themes"),
    staleTime: 0,
    refetchOnWindowFocus: true,
    refetchOnReconnect: true,
    refetchInterval: () => (document.hidden ? false : 15_000),
  });
}

export function useCreateTheme() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (data: CreateThemeInput) => api.post<Theme>("/themes", data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: themeKeys.all });
    },
  });
}

export function useUpdateTheme() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...data }: { id: string } & UpdateThemeInput) => api.patch<Theme>(`/themes/${id}`, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: themeKeys.all });
    },
  });
}

export function useDeleteTheme() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.delete(`/themes/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: themeKeys.all });
    },
  });
}

export function useSetActiveTheme() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string | null) => api.put<Theme | null>("/themes/active", { id }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: themeKeys.all });
    },
  });
}
