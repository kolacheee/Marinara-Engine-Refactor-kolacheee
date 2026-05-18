export interface UploadFilePayload {
  name: string;
  type: string;
  size: number;
  base64: string;
}

export async function fileToUploadPayload(file: File): Promise<UploadFilePayload> {
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return {
    name: file.name,
    type: file.type,
    size: file.size,
    base64: btoa(binary),
  };
}

export async function formDataToJson(body: FormData): Promise<Record<string, unknown>> {
  const entries: Record<string, unknown> = {};
  const appendEntry = (key: string, value: unknown) => {
    const existing = entries[key];
    if (existing === undefined) {
      entries[key] = value;
    } else if (Array.isArray(existing)) {
      existing.push(value);
    } else {
      entries[key] = [existing, value];
    }
  };
  for (const [key, value] of body.entries()) {
    appendEntry(key, value instanceof File ? await fileToUploadPayload(value) : value);
  }
  return entries;
}
