<script lang="ts">
  import type { Component } from 'svelte';

  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import BadgeCheck from '@lucide/svelte/icons/badge-check';
  import Check from '@lucide/svelte/icons/check';
  import CircleDot from '@lucide/svelte/icons/circle-dot';
  import DoorClosed from '@lucide/svelte/icons/door-closed';
  import DoorOpen from '@lucide/svelte/icons/door-open';
  import FileDown from '@lucide/svelte/icons/file-down';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import LogIn from '@lucide/svelte/icons/log-in';
  import LogOut from '@lucide/svelte/icons/log-out';
  import Power from '@lucide/svelte/icons/power';
  import PowerOff from '@lucide/svelte/icons/power-off';
  import Radio from '@lucide/svelte/icons/radio';
  import ClockAlert from '@lucide/svelte/icons/clock-alert';
  import ScrollText from '@lucide/svelte/icons/scroll-text';
  import Settings from '@lucide/svelte/icons/settings';
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import ShieldX from '@lucide/svelte/icons/shield-x';
  import SquareCheck from '@lucide/svelte/icons/square-check';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import WifiOff from '@lucide/svelte/icons/wifi-off';

  import {
    EVENT_CATEGORIES,
    type EventCategory,
    type EventKind,
    type RecordedEvent,
    fetchEvents,
  } from '$lib/api/journal';
  import { subscribeLive } from '$lib/api/live';
  import { formattingLocale, hour12, t } from '$lib/i18n/i18n.svelte';
  import { AUTO_PAGES, byDay, hasMore, newest, oldest, PAGE, prepend, toneOf } from '$lib/journal';
  import { formatClock } from '$lib/live';
  import Button from '$components/ui/Button.svelte';
  import Eyebrow from '$components/ui/Eyebrow.svelte';

  const ICONS: Record<EventKind, Component<{ size?: number; class?: string }>> = {
    'login-succeeded': LogIn,
    'login-failed': ShieldX,
    'login-throttled': ShieldAlert,
    'logged-out': LogOut,
    'credential-reset': KeyRound,
    started: Power,
    stopped: PowerOff,
    'configuration-changed': Settings,
    'service-opened': DoorOpen,
    'service-closed': DoorClosed,
    'stage-completed': Check,
    'calibration-started': CircleDot,
    'calibration-stopped': SquareCheck,
    'model-activated': BadgeCheck,
    'model-rejected': TriangleAlert,
    'node-appeared': Radio,
    'node-lost': WifiOff,
    'clock-stepped': ClockAlert,
    unknown: TriangleAlert,
  };

  const TONES = {
    fault: 'bg-danger/10 text-danger',
    attention: 'bg-density-low/15 text-density-medium',
    plain: 'bg-ink-100 text-ink-500',
  } as const;

  let family = $state<EventCategory | null>(null);
  let events = $state<RecordedEvent[]>([]);
  let arrived = $state<RecordedEvent[]>([]);
  let autoLoaded = $state(0);
  let more = $state(false);
  let busy = $state(false);
  let loaded = $state(false);
  let sentinel = $state<HTMLElement | null>(null);
  let liveId = $state<number | null>(null);

  /* Reloaded from scratch when the family changes: a filter is a different
     question, not a narrowing of the answer already on screen. */
  $effect(() => {
    const category = family;
    let cancelled = false;
    void (async () => {
      const first = await fetchEvents({ limit: PAGE, category });
      if (!cancelled) {
        events = first;
        arrived = [];
        autoLoaded = 0;
        more = hasMore(first);
        loaded = true;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  /* The live stream already ticks every second for the estimate, so it tells
     the journal that something happened without a second poll of its own. */
  $effect(() => {
    const close = subscribeLive((snapshot) => (liveId = snapshot.journal_id ?? null));
    return close;
  });

  $effect(() => {
    const since = newest(events);
    if (since === undefined || liveId === null || liveId <= since) {
      return;
    }
    const category = family;
    void fetchEvents({ limit: PAGE, after: since, category }).then((rows) => {
      arrived = rows;
    });
  });

  /* Loaded as the end of the list comes into view, up to a bound. Rendered
     rather than fetched on a scroll handler: the observer fires once, off the
     main thread, and costs nothing while the reader is elsewhere. */
  $effect(() => {
    const target = sentinel;
    if (!target || !more || autoLoaded >= AUTO_PAGES) {
      return;
    }
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        void loadMore(true);
      }
    });
    observer.observe(target);
    return () => observer.disconnect();
  });

  const days = $derived(byDay(events, formattingLocale()));
  const csvUrl = $derived(family ? `/api/events.csv?category=${family}` : '/api/events.csv');

  async function loadMore(automatic: boolean) {
    if (busy) {
      return;
    }
    busy = true;
    const older = await fetchEvents({ limit: PAGE, before: oldest(events), category: family });
    events = [...events, ...older];
    more = hasMore(older);
    if (automatic) {
      autoLoaded += 1;
    }
    busy = false;
  }

  function reveal() {
    events = prepend(arrived, events);
    arrived = [];
  }
</script>

<div class="max-w-3xl">
  <div class="mb-3 flex flex-wrap items-start justify-between gap-3">
    <p class="max-w-prose text-sm text-ink-500">{t('journal.lead')}</p>
    <Button variant="outline" size="sm" href={csvUrl} download>
      <FileDown size={13} aria-hidden="true" />{t('journal.export')}
    </Button>
  </div>

  <!-- Scrollable on a phone rather than wrapped: five chips on two lines read
       as two groups of filters instead of one. The vertical padding is not
       decoration: `overflow-x: auto` makes the cross axis scroll too, so a
       ring drawn outside the border box is clipped without room for it. -->
  <div class="-mx-4 -my-1 mb-2 overflow-x-auto px-4 py-1 sm:mx-0 sm:px-0">
    <div class="flex w-max gap-1.5">
      {#each [null, ...EVENT_CATEGORIES] as choice (choice ?? 'all')}
        <button
          type="button"
          onclick={() => (family = choice)}
          aria-pressed={family === choice}
          class="h-8 rounded-md px-3 text-xs font-medium whitespace-nowrap transition-colors
                 {family === choice
            ? 'bg-mariam-50 text-mariam-600 ring-1 ring-mariam-200'
            : 'bg-white text-ink-500 ring-1 ring-ink-200 hover:text-ink-900'}"
        >
          {choice ? t(`journal.${choice}` as const) : t('journal.all')}
        </button>
      {/each}
    </div>
  </div>

  {#if arrived.length > 0}
    <button
      type="button"
      onclick={reveal}
      class="mb-3 flex w-full items-center justify-center gap-1.5 rounded-md bg-mariam-50 px-3
             py-2 text-xs font-medium text-mariam-600 ring-1 ring-mariam-200
             transition-colors hover:bg-mariam-100"
    >
      <ArrowUp size={13} aria-hidden="true" />
      {arrived.length === 1
        ? t('journal.arrivedOne')
        : t('journal.arrived', { count: arrived.length })}
    </button>
  {/if}

  {#if !loaded}
    <p class="text-sm text-ink-500">{t('app.loading')}</p>
  {:else if events.length === 0}
    <div class="rounded-md bg-white px-4 py-10 text-center ring-1 ring-ink-200">
      <ScrollText size={20} class="mx-auto text-ink-300" aria-hidden="true" />
      <p class="mt-2 text-sm font-medium text-ink-900">{t('journal.empty')}</p>
      <p class="mx-auto mt-1 max-w-xs text-xs text-ink-500">{t('journal.emptyLead')}</p>
    </div>
  {:else}
    <div class="overflow-hidden rounded-md bg-white ring-1 ring-ink-200">
      {#each days as day (day.key)}
        <div class="border-b border-ink-100 bg-ink-50 px-4 py-2">
          <Eyebrow>{day.key}</Eyebrow>
        </div>
        <ul class="divide-y divide-ink-100">
          {#each day.events as event (event.id)}
            {@const Icon = ICONS[event.kind]}
            {@const tone = toneOf(event.kind)}
            <li class="flex items-start gap-3 px-4 py-3 sm:py-2.5">
              <span
                class="mt-px flex size-6 shrink-0 items-center justify-center rounded-md
                       {TONES[tone]}"
              >
                <Icon size={13} />
              </span>
              <span class="min-w-0 flex-1">
                <span class="flex flex-wrap items-baseline gap-x-2">
                  <span class="font-mono text-xs tabular-nums text-ink-900">
                    {formatClock(event.ts_us, formattingLocale(), hour12())}
                  </span>
                  <span
                    class="text-sm {tone === 'fault'
                      ? 'font-medium text-danger'
                      : tone === 'attention'
                        ? 'font-medium text-ink-900'
                        : 'text-ink-900'}"
                  >
                    {t(`event.${event.kind}` as const)}
                  </span>
                </span>
                {#if event.detail || event.client}
                  <span class="mt-0.5 block text-xs break-words text-ink-500">
                    {event.detail ?? ''}
                    {#if event.client}
                      <span class="font-mono">{event.client}</span>
                    {/if}
                  </span>
                {/if}
              </span>
            </li>
          {/each}
        </ul>
      {/each}
    </div>

    {#if more}
      <div bind:this={sentinel} class="mt-3 flex justify-center">
        {#if autoLoaded >= AUTO_PAGES}
          <Button variant="outline" size="sm" disabled={busy} onclick={() => void loadMore(false)}>
            {busy ? t('app.loading') : t('journal.more', { count: PAGE })}
          </Button>
        {:else}
          <span class="flex items-center gap-1 py-2" aria-live="polite">
            <span class="sr-only">{t('app.loading')}</span>
            {#each [0, 150, 300] as delay (delay)}
              <span
                class="size-1.5 animate-pulse rounded-full bg-ink-300"
                style="animation-delay: {delay}ms"
              ></span>
            {/each}
          </span>
        {/if}
      </div>
    {:else}
      <p class="mt-3 text-center text-xs text-ink-500">{t('journal.end')}</p>
    {/if}
  {/if}
</div>
