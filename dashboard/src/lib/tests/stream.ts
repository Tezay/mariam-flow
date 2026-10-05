import { type LiveSnapshot } from '$lib/api/live';

/** A live stream that says only what a test makes it say. */
export class Stream {
  static opened: Stream[] = [];

  onmessage: ((event: { data: string }) => void) | null = null;
  closed = false;

  constructor() {
    Stream.opened.push(this);
  }

  static get current(): Stream {
    return Stream.opened[Stream.opened.length - 1];
  }

  close() {
    this.closed = true;
  }

  say(snapshot: LiveSnapshot) {
    this.onmessage?.({ data: JSON.stringify(snapshot) });
  }
}

export const SNAPSHOT: LiveSnapshot = {
  stream: {
    running: true,
    estimating: false,
    frames: 0,
    estimates: 0,
    edge_stamped: true,
    nodes: {},
  },
  service: { open: true },
  recording: false,
  now_us: 1_785_600_000_000_000,
};
