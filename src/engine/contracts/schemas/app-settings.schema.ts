// ──────────────────────────────────────────────
// App Settings Zod Schemas
// ──────────────────────────────────────────────
import { z } from "zod";

/** Payload for writing an opaque serialized app settings blob. */
export const appSettingsUpdateSchema = z.object({
  value: z.string().max(1_000_000),
});

/** Response shape for reading an app settings blob. */
export const appSettingsResponseSchema = z.object({
  value: z.string().nullable(),
});

export type AppSettingsUpdateInput = z.infer<typeof appSettingsUpdateSchema>;
export type AppSettingsResponse = z.infer<typeof appSettingsResponseSchema>;
