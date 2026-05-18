// ──────────────────────────────────────────────
// Hooks: Custom Themes
// ──────────────────────────────────────────────
import { useEffect, useRef } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../../shared/lib/api-client";
import { useUIStore } from "../../../shared/stores/ui.store";
import type { CreateThemeInput, Theme, UpdateThemeInput } from "@marinara-engine/shared";

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

export function useLegacyThemeMigration() {
  const legacyThemes = useUIStore((s) => s.customThemes);
  const legacyActiveCustomTheme = useUIStore((s) => s.activeCustomTheme);
  const hasMigratedCustomThemesToServer = useUIStore((s) => s.hasMigratedCustomThemesToServer);
  const clearLegacyCustomThemes = useUIStore((s) => s.clearLegacyCustomThemes);
  const setHasMigratedCustomThemesToServer = useUIStore((s) => s.setHasMigratedCustomThemesToServer);
  const qc = useQueryClient();
  const inFlightRef = useRef(false);
  const { isSuccess } = useThemes();

  useEffect(() => {
    if (hasMigratedCustomThemesToServer || !isSuccess || inFlightRef.current) {
      return;
    }

    inFlightRef.current = true;
    void (async () => {
      try {
        const latestThemes = await api.get<Theme[]>("/themes");
        const storedAlreadyHasActiveTheme = latestThemes.some((theme) => theme.isActive);
        let workingThemes = [...latestThemes];
        let migratedActiveThemeId: string | null = null;

        for (const legacyTheme of legacyThemes) {
          let storedTheme = findDuplicateTheme(workingThemes, legacyTheme.name, legacyTheme.css);
          if (!storedTheme) {
            storedTheme = await api.post<Theme>("/themes", {
              name: legacyTheme.name,
              css: legacyTheme.css,
              installedAt: legacyTheme.installedAt,
            });
            workingThemes = [storedTheme, ...workingThemes];
          }

          if (!storedAlreadyHasActiveTheme && legacyActiveCustomTheme === legacyTheme.id) {
            migratedActiveThemeId = storedTheme.id;
          }
        }

        if (migratedActiveThemeId) {
          await api.put<Theme | null>("/themes/active", { id: migratedActiveThemeId });
        }

        clearLegacyCustomThemes();
        setHasMigratedCustomThemesToServer(true);
        await qc.invalidateQueries({ queryKey: themeKeys.all });
      } catch {
        // Leave migration flag untouched so the next app start can retry.
      } finally {
        inFlightRef.current = false;
      }
    })();
  }, [
    clearLegacyCustomThemes,
    hasMigratedCustomThemesToServer,
    isSuccess,
    legacyActiveCustomTheme,
    legacyThemes,
    qc,
    setHasMigratedCustomThemesToServer,
  ]);
}
