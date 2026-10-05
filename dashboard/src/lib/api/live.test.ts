import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { reopenLive, subscribeLive } from './live';
import { SNAPSHOT, Stream } from '$lib/tests/stream';

describe('subscribeLive', () => {
  let stops: (() => void)[] = [];
  const listen = () => {
    const heard = vi.fn();
    stops.push(subscribeLive(heard));
    return heard;
  };

  beforeEach(() => {
    Stream.opened = [];
    vi.stubGlobal('EventSource', Stream);
  });
  afterEach(() => {
    stops.forEach((stop) => stop());
    stops = [];
    vi.unstubAllGlobals();
  });

  it('serves every subscriber from one connection', () => {
    const [first, second] = [listen(), listen()];

    Stream.current.say(SNAPSHOT);

    expect(Stream.opened).toHaveLength(1);
    expect(first).toHaveBeenCalledWith(SNAPSHOT);
    expect(second).toHaveBeenCalledWith(SNAPSHOT);
  });

  it('hands a late subscriber the last snapshot', () => {
    listen();
    Stream.current.say(SNAPSHOT);

    expect(listen()).toHaveBeenCalledWith(SNAPSHOT);
  });

  it('closes the connection with its last subscriber, and remembers nothing', () => {
    listen();
    Stream.current.say(SNAPSHOT);
    stops.forEach((stop) => stop());
    stops = [];
    expect(Stream.current.closed).toBe(true);

    expect(listen()).not.toHaveBeenCalled();
    expect(Stream.opened).toHaveLength(2);
  });

  it('survives a frame it cannot read', () => {
    const heard = listen();

    Stream.current.onmessage?.({ data: 'not json' });
    Stream.current.say(SNAPSHOT);

    expect(heard).toHaveBeenCalledOnce();
  });

  it('replaces the connection without losing its subscribers', () => {
    const heard = listen();
    const first = Stream.current;

    reopenLive();
    Stream.current.say(SNAPSHOT);

    expect(first.closed).toBe(true);
    expect(Stream.opened).toHaveLength(2);
    expect(heard).toHaveBeenCalledWith(SNAPSHOT);
  });

  it('opens nothing when nobody is listening', () => {
    reopenLive();

    expect(Stream.opened).toHaveLength(0);
  });
});
