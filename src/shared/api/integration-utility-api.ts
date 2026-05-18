import { api } from "./api-client";

export interface GifSearchResult {
  id: string;
  title: string;
  preview: string;
  url: string;
  width: number;
  height: number;
}

export interface GifSearchResponse {
  results: GifSearchResult[];
  next: string;
}

export interface SpotifyStatus {
  connected: boolean;
  expired?: boolean;
  redirectUri?: string | null;
}

export interface SpotifyAuthorizeResponse {
  authUrl?: string;
  error?: string;
  [key: string]: unknown;
}

export interface SpotifyExchangeResponse {
  success?: boolean;
  error?: string;
  [key: string]: unknown;
}

interface TtsSpeakResponse {
  audioBase64?: string;
  base64?: string;
  audio?: string;
  contentType?: string;
  mimeType?: string;
  ok?: boolean;
  message?: string;
  error?: string;
}

function base64ToBlob(base64: string, contentType: string): Blob {
  const binary = atob(base64);
  const chunks: ArrayBuffer[] = [];
  for (let offset = 0; offset < binary.length; offset += 8192) {
    const slice = binary.slice(offset, offset + 8192);
    const bytes = new Uint8Array(slice.length);
    for (let index = 0; index < slice.length; index += 1) {
      bytes[index] = slice.charCodeAt(index);
    }
    chunks.push(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
  }
  return new Blob(chunks, { type: contentType });
}

export const gifsApi = {
  search: (input: { q?: string; limit?: number; pos?: string }) => {
    const params = new URLSearchParams({ limit: String(input.limit ?? 20) });
    if (input.q?.trim()) params.set("q", input.q.trim());
    if (input.pos) params.set("pos", input.pos);
    return api.get<GifSearchResponse>(`/gifs/search?${params}`);
  },
};

export const ttsApi = {
  speak: async (
    input: { text: string; speaker?: string; tone?: string; voice?: string },
    signal?: AbortSignal,
  ): Promise<Blob> => {
    const response = await api.post<TtsSpeakResponse>("/tts/speak", input, { signal });
    const audio = response.audioBase64 ?? response.base64 ?? response.audio;
    if (!audio) {
      throw new Error(response.error ?? response.message ?? "TTS request did not return audio.");
    }
    return base64ToBlob(audio, response.contentType ?? response.mimeType ?? "audio/mpeg");
  },
};

export const spotifyApi = {
  status: (agentId: string) => api.post<SpotifyStatus>("/spotify/status", { agentId }),
  authorize: (input: { clientId: string; agentId: string }) =>
    api.post<SpotifyAuthorizeResponse>("/spotify/authorize", input),
  exchange: (callbackUrl: string) => api.post<SpotifyExchangeResponse>("/spotify/exchange", { callbackUrl }),
  disconnect: (agentId: string) => api.post("/spotify/disconnect", { agentId }),
};

export const knowledgeSourcesApi = {
  upload: (file: File) => {
    const form = new FormData();
    form.append("file", file);
    return api.upload("/knowledge-sources/upload", form);
  },
};

export const connectionsUtilityApi = {
  list: <T = unknown>() => api.get<T>("/connections"),
};
