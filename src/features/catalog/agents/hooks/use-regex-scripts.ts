// ──────────────────────────────────────────────
// Hooks: Regex Scripts (React Query)
// ──────────────────────────────────────────────
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { storageApi } from "../../../../shared/api/storage-api";

const regexKeys = {
  all: ["regex-scripts"] as const,
  detail: (id: string) => ["regex-scripts", id] as const,
};

export interface RegexScriptRow {
  id: string;
  characterId: string | null;
  name: string;
  enabled: string;
  findRegex: string;
  replaceString: string;
  trimStrings: string;
  placement: string;
  flags: string;
  promptOnly: string;
  order: number;
  minDepth: number | null;
  maxDepth: number | null;
  createdAt: string;
  updatedAt: string;
}

export function useRegexScripts(characterIds?: string[]) {
  return useQuery({
    queryKey: characterIds?.length ? [...regexKeys.all, ...characterIds] : regexKeys.all,
    queryFn: async () => {
      const all = await storageApi.list<RegexScriptRow>("regex-scripts");
      if (!characterIds?.length) {
        // No character context: return only global scripts
        return all.filter((s) => !s.characterId);
      }
      // Return global scripts + scripts scoped to the given characters
      const idSet = new Set(characterIds);
      return all.filter((s) => !s.characterId || idSet.has(s.characterId));
    },
  });
}

export function useRegexScript(id: string | null) {
  return useQuery({
    queryKey: regexKeys.detail(id ?? ""),
    queryFn: () => storageApi.get<RegexScriptRow>("regex-scripts", id!),
    enabled: !!id,
  });
}

export function useCreateRegexScript() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (data: Record<string, unknown>) => storageApi.create<RegexScriptRow>("regex-scripts", data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: regexKeys.all });
    },
  });
}

export function useUpdateRegexScript() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...data }: { id: string } & Record<string, unknown>) =>
      storageApi.update<RegexScriptRow>("regex-scripts", id, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: regexKeys.all });
    },
  });
}

export function useReorderRegexScripts() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (scriptIds: string[]) => {
      await Promise.all(
        scriptIds.map((id, index) => storageApi.update("regex-scripts", id, { sortOrder: index, order: index })),
      );
      return storageApi.list<RegexScriptRow>("regex-scripts");
    },
    onSuccess: (scripts) => {
      qc.setQueryData(regexKeys.all, scripts);
      qc.invalidateQueries({ queryKey: regexKeys.all });
    },
  });
}

export function useDeleteRegexScript() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => storageApi.delete("regex-scripts", id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: regexKeys.all });
    },
  });
}

export function useBatchCreateRegexScripts() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (scripts: Array<Record<string, unknown>>) => {
      const results = await Promise.all(
        scripts.map((data) => storageApi.create<RegexScriptRow>("regex-scripts", data)),
      );
      return results;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: regexKeys.all });
    },
  });
}

export function useDeleteRegexScriptsByCharacter() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (characterId: string) => {
      const all = await storageApi.list<RegexScriptRow>("regex-scripts");
      const toDelete = all.filter((s) => s.characterId === characterId);
      await Promise.all(toDelete.map((s) => storageApi.delete("regex-scripts", s.id)));
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: regexKeys.all });
    },
  });
}
