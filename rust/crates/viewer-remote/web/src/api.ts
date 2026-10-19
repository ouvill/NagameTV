import { createClient, ConnectError, Code } from "@connectrpc/connect";
import { createGrpcWebTransport } from "@connectrpc/connect-web";
import { PlayerService, type Channel, type PlayerState } from "./gen/viewer/v1/player_pb";

// Same origin as the page: the viewer serves both, so no CORS is involved.
const transport = createGrpcWebTransport({ baseUrl: location.origin, useBinaryFormat: true });
export const client = createClient(PlayerService, transport);

export type Listener = {
  state(state: PlayerState): void;
  channels(channels: Channel[]): void;
  connection(status: "connecting" | "connected" | "retrying"): void;
};

const MAX_BACKOFF_MS = 8000;
// Playback position also bumps the shared revision, so listing on every bump would
// re-fetch the whole catalog several times per second.
const CHANNEL_REFRESH_MS = 10_000;

let channelsFetchedAt = 0;
let channelsListener: Listener["channels"] | undefined;

export async function refreshChannels(): Promise<void> {
  channelsFetchedAt = Date.now();
  const channels = (await client.listChannels({})).channels;
  channelsListener?.(channels);
}

/// Keeps one WatchState stream open, restoring the full state after reconnects.
export async function watch(listener: Listener): Promise<never> {
  channelsListener = listener.channels;
  let backoff = 500;
  for (;;) {
    const abort = new AbortController();
    let connected = false;
    try {
      for await (const response of client.watchState({}, { signal: abort.signal })) {
        const state = response.state;
        if (!state) continue;
        if (!connected) {
          connected = true;
          backoff = 500;
          listener.connection("connected");
          await refreshChannels();
        } else if (Date.now() - channelsFetchedAt >= CHANNEL_REFRESH_MS) {
          await refreshChannels();
        }
        listener.state(state);
      }
    } catch (error) {
      console.warn("WatchState ended", error);
    }
    abort.abort();
    listener.connection("retrying");
    await new Promise((resolve) => setTimeout(resolve, backoff));
    backoff = Math.min(backoff * 2, MAX_BACKOFF_MS);
  }
}

/// Fire-and-forget form for UI handlers; failures go to the supplied reporter.
export function run(action: () => Promise<unknown>, report: (message: string) => void): void {
  action().catch((error: unknown) => {
    console.warn("Remote command failed", error);
    report(describe(error));
  });
}

export function describe(error: unknown): string {
  const connect = ConnectError.from(error);
  // Precondition failures carry the viewer's own explanation; others are generic.
  return connect.code === Code.FailedPrecondition || connect.code === Code.NotFound
    ? connect.rawMessage
    : `${Code[connect.code]}: ${connect.rawMessage}`;
}
