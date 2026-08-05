import { describe, expect, it } from 'vitest';

import { type ClassMapping } from './api/status';
import {
  GRACE_SECONDS,
  defaultEnvironment,
  formatDay,
  formatWindow,
  modelName,
  queueComplete,
  blankClasses,
  buttonLabel,
  buttonOrder,
  formatElapsed,
  isDiscardingFrames,
  secondsBetween,
} from './calibration';

const DESCRIBED: ClassMapping = {
  empty: 'personne',
  low: 'quelques personnes',
  medium: '',
  saturated: 'file au-delà de la porte',
};

describe('buttonLabel', () => {
  it('prefers the words the site chose', () => {
    expect(buttonLabel('saturated', DESCRIBED, 'Saturated')).toBe('file au-delà de la porte');
  });

  it('falls back to the class name when nothing was described', () => {
    // A blank description must not leave a blank button.
    expect(buttonLabel('medium', DESCRIBED, 'Medium')).toBe('Medium');
    expect(buttonLabel('low', null, 'Low')).toBe('Low');
    expect(buttonLabel('low', blankClasses(), 'Low')).toBe('Low');
  });

  it('ignores a description that is only spaces', () => {
    expect(buttonLabel('empty', { ...DESCRIBED, empty: '   ' }, 'Empty')).toBe('Empty');
  });
});

describe('formatElapsed', () => {
  it('counts seconds in the first minute', () => {
    expect(formatElapsed(0)).toBe('0 s');
    expect(formatElapsed(59.6)).toBe('59 s');
  });

  it('keeps the seconds visible within the hour', () => {
    // A labeller watches this to judge how long the queue has held a state;
    // dropping the seconds would make the first minutes unreadable.
    expect(formatElapsed(60)).toBe('1 min 0 s');
    expect(formatElapsed(605)).toBe('10 min 5 s');
  });

  it('switches to hours on a long capture', () => {
    expect(formatElapsed(3600)).toBe('1 h 0 min');
    expect(formatElapsed(7860)).toBe('2 h 11 min');
  });

  it('never counts backwards', () => {
    expect(formatElapsed(-5)).toBe('0 s');
  });
});

describe('secondsBetween', () => {
  it('measures on the appliance clock', () => {
    expect(secondsBetween(1_000_000, 4_500_000)).toBe(3.5);
  });

  it('reports nothing rather than a negative span', () => {
    // The two timestamps come from the same clock, but a status arriving out
    // of order must not make the elapsed time run backwards on screen.
    expect(secondsBetween(4_000_000, 1_000_000)).toBe(0);
  });
});

describe('isDiscardingFrames', () => {
  it('says nothing while the capture is still being set up', () => {
    expect(isDiscardingFrames(false, 0)).toBe(false);
    expect(isDiscardingFrames(false, GRACE_SECONDS - 1)).toBe(false);
  });

  it('warns once an unlabelled capture has been running', () => {
    // Windows before the first label carry no class and are dropped: the
    // capture is recording nothing usable, and nothing else would say so.
    expect(isDiscardingFrames(false, GRACE_SECONDS)).toBe(true);
  });

  it('stays quiet once a class has been marked', () => {
    expect(isDiscardingFrames(true, 3600)).toBe(false);
  });
});

describe('buttonOrder', () => {
  it('puts the densest class nearest the thumb', () => {
    expect(buttonOrder()).toEqual(['saturated', 'medium', 'low', 'empty']);
  });

  it('offers every class', () => {
    expect(new Set(buttonOrder()).size).toBe(4);
  });
});

describe('defaultEnvironment', () => {
  const AT = new Date(2026, 6, 31, 12, 30);

  it('names the site and when it is', () => {
    const proposed = defaultEnvironment('RU Efrei', AT, 'fr-FR');
    expect(proposed.startsWith('RU Efrei — ')).toBe(true);
    expect(proposed).toMatch(/\d/);
  });

  it('still proposes something for a site with no name', () => {
    expect(defaultEnvironment(null, AT, 'fr-FR').length).toBeGreaterThan(0);
  });
});

describe('modelName', () => {
  it('prefers the name the training run declared', () => {
    expect(modelName({ id: '20260801T120000Z-x', manifest: { name: 'campagne-juin' } })).toBe(
      'campagne-juin',
    );
  });

  it('falls back to the handle for an anonymous bundle', () => {
    // A bundle predating the manifest still has to be nameable on screen.
    expect(modelName({ id: '20260801T120000Z-model' })).toBe('20260801T120000Z-model');
    expect(modelName({ id: 'handle', manifest: { name: '  ' } })).toBe('handle');
  });
});

describe('formatWindow', () => {
  it('speaks in seconds, which is how a window is discussed', () => {
    expect(formatWindow(5_000_000)).toBe('5');
    expect(formatWindow(2_500_000)).toBe('2.5');
  });
});

describe('formatDay', () => {
  it('spells the day out rather than abbreviating it', () => {
    const rendered = formatDay(Date.UTC(2026, 7, 12, 10) * 1000, 'fr-FR');
    expect(rendered).toMatch(/\p{L}{4,}/u);
  });
});

describe('queueComplete', () => {
  it('needs every count and a service rate', () => {
    expect(queueComplete([0, 4, 12, 25], 6)).toBe(true);
    expect(queueComplete([0, 4, 12, null], 6)).toBe(false);
    expect(queueComplete([0, 4, 12, 25], null)).toBe(false);
  });

  it('refuses what the appliance would refuse', () => {
    // A queue served at zero people per minute never empties, and the
    // estimator says so rather than dividing by it.
    expect(queueComplete([0, 4, 12, 25], 0)).toBe(false);
    expect(queueComplete([0, -1, 12, 25], 6)).toBe(false);
  });

  it('needs one count per density class', () => {
    expect(queueComplete([0, 4, 12], 6)).toBe(false);
  });
});
