import { invoke } from "@tauri-apps/api/core";
import { writable } from "svelte/store";

type Playback = { id: string; owner: string; playing: boolean; error: boolean };
export const playback = writable<Playback | null>(null);
let current: Playback | null = null;
let queue = Promise.resolve();
function publish(value: Playback | null) {
  current = value;
  playback.set(value);
}
function enqueue(action: () => Promise<void>) {
  queue = queue.then(action).catch(() => {
    /* Each request reports its own failure. */
  });
}
export function stopReading(owner: string) {
  const request = current;
  if (request?.owner !== owner) return;
  publish(null);
  enqueue(async () => {
    await invoke("stop_speech", { requestId: request.id });
  });
}
export function readAloud(
  owner: string,
  text: string,
  language: string | null,
) {
  const previous = current;
  const request: Playback = {
    id: crypto.randomUUID(),
    owner,
    playing: true,
    error: false,
  };
  publish(request);
  enqueue(async () => {
    try {
      if (previous?.playing)
        await invoke("stop_speech", { requestId: previous.id });
      if (current?.id !== request.id) return;
      await invoke("speak_text", { requestId: request.id, text, language });
      if (current?.id === request.id) void poll(request);
    } catch {
      if (current?.id === request.id)
        publish({ ...request, playing: false, error: true });
    }
  });
}
async function poll(request: Playback) {
  while (current?.id === request.id && current.playing) {
    try {
      const playing = await invoke<boolean>("speech_status", {
        requestId: request.id,
      });
      if (current?.id !== request.id) return;
      if (!playing) {
        publish(null);
        return;
      }
    } catch {
      if (current?.id === request.id)
        publish({ ...request, playing: false, error: true });
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}
