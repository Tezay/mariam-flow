// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { type Status } from './api/status';
import {
  LOST_AFTER_MS,
  PROBE_EVERY_MS,
  connectionLost,
  lastHeardAt,
  watchConnection,
} from './connection.svelte';
import { SNAPSHOT, Stream } from '$lib/tests/stream';

const STATUS = { kit_id: 'KIT-0042' } as Status;

const answers = (status: number, body: unknown = null) =>
  vi.fn(() => Promise.resolve(new Response(JSON.stringify(body), { status })));

/** An appliance that is off: nothing comes back until the caller gives up. */
const silent = () =>
  vi.fn(
    (_url: string, init: RequestInit) =>
      new Promise<Response>((_, reject) =>
        init.signal?.addEventListener('abort', () => reject(new Error('aborted'))),
      ),
  );

describe('watchConnection', () => {
  let stop: () => void = () => {};
  const handlers = { onanswer: vi.fn(), onunauthorized: vi.fn() };
  const watch = () => (stop = watchConnection(handlers));
  const pass = (ms: number) => vi.advanceTimersByTimeAsync(ms);

  beforeEach(() => {
    vi.useFakeTimers();
    Stream.opened = [];
    vi.stubGlobal('EventSource', Stream);
    vi.stubGlobal('fetch', silent());
  });
  afterEach(() => {
    stop();
    vi.clearAllMocks();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('holds the appliance present for as long as the stream speaks', async () => {
    watch();

    for (let second = 0; second < 10; second += 1) {
      Stream.current.say(SNAPSHOT);
      await pass(1_000);
    }

    expect(connectionLost()).toBe(false);
    expect(fetch).not.toHaveBeenCalled();
  });

  it('reports it gone once the stream has been quiet for too long', async () => {
    watch();
    Stream.current.say(SNAPSHOT);
    const heard = Date.now();

    await pass(LOST_AFTER_MS);
    expect(connectionLost()).toBe(false);
    await pass(1_000);

    expect(connectionLost()).toBe(true);
    expect(lastHeardAt()).toBe(heard);
  });

  it('reports an appliance that never spoke at all', async () => {
    watch();

    await pass(LOST_AFTER_MS + 1_000);

    expect(connectionLost()).toBe(true);
    expect(lastHeardAt()).toBeNull();
  });

  it('comes back on its own when the stream speaks again', async () => {
    watch();
    await pass(LOST_AFTER_MS + 1_000);

    Stream.current.say(SNAPSHOT);

    expect(connectionLost()).toBe(false);
  });

  it('reopens the stream once the appliance answers again', async () => {
    // The connection that went quiet is closed or waiting on nobody; only a
    // new one will be spoken to.
    vi.stubGlobal('fetch', answers(200, STATUS));
    watch();
    const quiet = Stream.current;

    await pass(LOST_AFTER_MS + 1_000);

    expect(handlers.onanswer).toHaveBeenCalledWith(STATUS);
    expect(quiet.closed).toBe(true);
    expect(Stream.opened).toHaveLength(2);
    expect(connectionLost()).toBe(true);
  });

  it('hands over when the appliance no longer knows the session', async () => {
    vi.stubGlobal('fetch', answers(401));
    watch();

    await pass(LOST_AFTER_MS + 1_000);

    expect(handlers.onunauthorized).toHaveBeenCalledOnce();
    expect(Stream.opened).toHaveLength(1);
  });

  it('gives up on a question nobody answers, and asks again', async () => {
    watch();

    await pass(LOST_AFTER_MS + 1_000 + 2 * PROBE_EVERY_MS);

    expect(vi.mocked(fetch).mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(handlers.onanswer).not.toHaveBeenCalled();
  });

  it('counts the silence of a tab from its return to the foreground', async () => {
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
    watch();
    await pass(LOST_AFTER_MS + 1_000);

    document.dispatchEvent(new Event('visibilitychange'));
    expect(connectionLost()).toBe(false);

    await pass(LOST_AFTER_MS + 1_000);
    expect(connectionLost()).toBe(true);
  });

  it('acts on no answer that arrives after it stopped watching', async () => {
    let answer: (response: Response) => void = () => {};
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise<Response>((resolve) => (answer = resolve))),
    );
    watch();
    await pass(LOST_AFTER_MS + 1_000);

    stop();
    answer(new Response(JSON.stringify(STATUS), { status: 200 }));
    await pass(0);

    expect(handlers.onanswer).not.toHaveBeenCalled();
    expect(connectionLost()).toBe(false);
  });
});
