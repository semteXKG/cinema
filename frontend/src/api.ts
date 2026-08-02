import type { ApiPayload } from "./types";

export async function fetchShowings(): Promise<ApiPayload> {
  const resp = await fetch("/api/showings");
  if (!resp.ok) {
    throw new Error(`GET /api/showings failed: ${resp.status}`);
  }
  return (await resp.json()) as ApiPayload;
}
