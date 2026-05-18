import type { StorageGateway, StorageListOptions } from "../capabilities/storage";

export interface EntityRepository<T extends { id: string }> {
  list(options?: StorageListOptions): Promise<T[]>;
  get(id: string): Promise<T | null>;
  create(value: Partial<T> & Record<string, unknown>): Promise<T>;
  update(id: string, patch: Partial<T> & Record<string, unknown>): Promise<T>;
  delete(id: string): Promise<{ deleted: boolean }>;
}

export function createEntityRepository<T extends { id: string }>(
  storage: StorageGateway,
  entity: string,
): EntityRepository<T> {
  return {
    list: (options) => storage.list<T>(entity, options),
    get: (id) => storage.get<T>(entity, id),
    create: (value) => storage.create<T>(entity, value),
    update: (id, patch) => storage.update<T>(entity, id, patch),
    delete: (id) => storage.delete(entity, id),
  };
}
