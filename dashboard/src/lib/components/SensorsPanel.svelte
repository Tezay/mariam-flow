<script lang="ts">
  import ArrowLeftRight from '@lucide/svelte/icons/arrow-left-right';
  import MapPin from '@lucide/svelte/icons/map-pin';
  import Radio from '@lucide/svelte/icons/radio';

  import {
    adoptHardware,
    describeNode,
    fetchDiscovery,
    subscribeLive,
    type Discovery,
    type LiveSnapshot,
    type SensingNode,
    type Status,
  } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
  import {
    receiverState,
    replacements,
    transmitterHeard,
    transmitterReplacement,
  } from '$lib/sensors';
  import Button from '$components/ui/Button.svelte';
  import Eyebrow from '$components/ui/Eyebrow.svelte';
  import Modal from '$components/ui/Modal.svelte';
  import NameDialog from '$components/ui/NameDialog.svelte';
  import SectionHeader from '$components/ui/SectionHeader.svelte';

  let { status, onupdated }: { status: Status; onupdated: (status: Status) => void } = $props();

  /** How often the offer is refreshed while a replacement is being chosen. */
  const POLL_MS = 2000;

  let snapshot = $state<LiveSnapshot | null>(null);
  let discovery = $state<Discovery | null>(null);
  let placing = $state<SensingNode | null>(null);
  let replacing = $state<SensingNode | null>(null);
  let busy = $state(false);
  let failure = $state<string | null>(null);

  $effect(() => {
    const close = subscribeLive((next) => (snapshot = next));
    return close;
  });

  /* Only polled while a replacement is being chosen: the rest of the time
     nobody is looking at what else the appliance can hear. */
  $effect(() => {
    if (!replacing) {
      return;
    }
    let cancelled = false;
    const load = async () => {
      const next = await fetchDiscovery();
      if (!cancelled && next) {
        discovery = next;
      }
    };
    void load();
    const timer = setInterval(() => void load(), POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  });

  const health = $derived(snapshot?.stream.nodes ?? {});
  const transmitter = $derived(status.nodes.find((node) => node.role === 'tx') ?? null);
  const receivers = $derived(status.nodes.filter((node) => node.role === 'rx'));
  const heard = $derived(transmitterHeard(status.nodes, health));
  const offered = $derived(replacements(discovery, status.nodes));
  const newTransmitter = $derived(transmitterReplacement(discovery, transmitter));

  /** The colour a status dot takes: settled, watch, or wrong. */
  const DOT = {
    good: 'bg-density-empty',
    warn: 'bg-density-low',
    bad: 'bg-density-saturated',
  } as const;

  function reading(node: SensingNode) {
    if (node.role === 'tx') {
      return heard
        ? { tone: 'good' as const, text: t('sensors.heard'), faulty: false }
        : { tone: 'warn' as const, text: t('sensors.notHeard'), faulty: true };
    }
    const rx = receiverState(health[node.node_id], snapshot?.stream, snapshot?.now_us);
    if (rx.kind === 'streaming') {
      return {
        tone: 'good' as const,
        text: t('live.framesPerSecond', { value: rx.framesPerSecond.toFixed(0) }),
        faulty: false,
      };
    }
    if (rx.kind === 'silent') {
      return {
        tone: 'bad' as const,
        text: t('live.silent', { seconds: rx.seconds }),
        faulty: true,
      };
    }
    return { tone: 'warn' as const, text: t('sensors.neverHeard'), faulty: true };
  }

  function accept(outcome: Awaited<ReturnType<typeof describeNode>>) {
    busy = false;
    if (outcome.kind === 'ok') {
      failure = null;
      onupdated(outcome.status);
      return true;
    }
    failure =
      outcome.kind === 'refused'
        ? t('wizard.refused', { message: outcome.message })
        : t('wizard.failed');
    return false;
  }

  async function place(position: string) {
    if (!placing) {
      return;
    }
    busy = true;
    accept(await describeNode(placing.node_id, position));
    placing = null;
  }

  async function adopt(address: string) {
    if (!replacing) {
      return;
    }
    busy = true;
    const hardware = replacing.role === 'tx' ? { mac: address } : { address };
    if (accept(await adoptHardware(replacing.node_id, hardware))) {
      replacing = null;
    }
  }
</script>

{#snippet row(node: SensingNode)}
  {@const shown = reading(node)}
  <li class="flex items-center gap-3 px-4 py-3 sm:py-2.5">
    <span class="min-w-0 flex-1">
      <span class="flex items-baseline gap-2">
        <span class="size-1.5 shrink-0 self-center rounded-full {DOT[shown.tone]}"></span>
        <span class="font-mono text-sm text-ink-900">{node.node_id}</span>
        <span class="truncate font-mono text-xs text-ink-500">
          {node.address ?? node.mac ?? ''}
        </span>
      </span>
      <span class="mt-0.5 ml-3.5 block truncate text-xs">
        <span class={node.position ? 'text-ink-700' : 'text-ink-300'}>
          {node.position || t('sensors.noPosition')}
        </span>
        <span class="text-ink-300"> · </span>
        <span class={shown.faulty ? 'text-density-saturated' : 'text-ink-500'}>
          {shown.text}
        </span>
      </span>
    </span>

    <!-- Always present rather than revealed on hover: this appliance is
         operated from a phone, where there is no hover to reveal them. -->
    <span class="flex shrink-0 items-center gap-0.5">
      <Button
        variant="quiet"
        size="icon"
        label={t('sensors.place')}
        title={t('sensors.place')}
        onclick={() => (placing = node)}
      >
        <MapPin size={15} aria-hidden="true" />
      </Button>
      <Button
        variant="quiet"
        size="icon"
        label={t('sensors.replace')}
        title={t('sensors.replace')}
        onclick={() => (replacing = node)}
      >
        <ArrowLeftRight size={15} aria-hidden="true" />
      </Button>
    </span>
  </li>
{/snippet}

<div class="max-w-3xl">
  <SectionHeader icon={Radio} title={t('sensors.title')} lead={t('sensors.lead')} />

  {#if failure}
    <p role="status" class="mb-3 text-sm text-danger">{failure}</p>
  {/if}

  {#if status.nodes.length === 0}
    <div class="rounded-md bg-white px-4 py-10 text-center ring-1 ring-ink-200">
      <Radio size={20} class="mx-auto text-ink-300" aria-hidden="true" />
      <p class="mt-2 text-sm font-medium text-ink-900">{t('nodes.none')}</p>
      <p class="mx-auto mt-1 max-w-xs text-xs text-ink-500">{t('sensors.noneLead')}</p>
    </div>
  {:else}
    <div class="overflow-hidden rounded-md bg-white ring-1 ring-ink-200">
      {#if transmitter}
        <div
          class="flex items-baseline justify-between gap-3 border-b border-ink-100 bg-ink-50
                 px-4 py-2"
        >
          <Eyebrow>{t('pair.transmitter')}</Eyebrow>
          <span class="truncate text-xs text-ink-500">{t('sensors.transmitterLead')}</span>
        </div>
        <ul>{@render row(transmitter)}</ul>
      {/if}

      <div
        class="flex items-baseline justify-between gap-3 border-y border-ink-100 bg-ink-50
               px-4 py-2"
      >
        <Eyebrow>{t('pair.receivers')}</Eyebrow>
        <span class="text-xs tabular-nums text-ink-500">{receivers.length}</span>
      </div>
      <ul class="divide-y divide-ink-100">
        {#each receivers as node (node.node_id)}
          {@render row(node)}
        {/each}
      </ul>
    </div>
  {/if}
</div>

{#if placing}
  <NameDialog
    title={t('sensors.placeTitle', { node: placing.node_id })}
    initial={placing.position ?? ''}
    allowEmpty
    onrename={(position) => void place(position)}
    oncancel={() => (placing = null)}
  />
{/if}

{#if replacing}
  <Modal
    title={t('sensors.replaceTitle', { node: replacing.node_id })}
    lead={t('sensors.replaceLead')}
    oncancel={() => (replacing = null)}
  >
    {#if replacing.role === 'tx' && newTransmitter}
      <ul class="-mt-2 rounded-md ring-1 ring-ink-200">
        <li class="flex items-center justify-between gap-3 px-3 py-2">
          <span class="min-w-0">
            <span class="block truncate font-mono text-sm text-ink-900">{newTransmitter.mac}</span>
            <span class="block text-xs text-ink-500">
              {t('pair.transmitterAgreed', { count: newTransmitter.agreement })}
            </span>
          </span>
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            onclick={() => void adopt(newTransmitter.mac)}
          >
            {t('sensors.adopt')}
          </Button>
        </li>
      </ul>
    {:else if replacing.role === 'rx' && offered.length > 0}
      <ul class="-mt-2 divide-y divide-ink-100 rounded-md ring-1 ring-ink-200">
        {#each offered as candidate (candidate.address)}
          <li class="flex items-center justify-between gap-3 px-3 py-2">
            <span class="min-w-0">
              <span class="block truncate font-mono text-sm text-ink-900">{candidate.address}</span>
              <span class="block text-xs text-ink-500">
                {t('pair.rate', { value: candidate.datagrams_per_second.toFixed(0) })}
              </span>
            </span>
            <Button
              variant="outline"
              size="sm"
              disabled={busy}
              onclick={() => void adopt(candidate.address)}
            >
              {t('sensors.adopt')}
            </Button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="-mt-2 text-sm text-ink-500">
        {replacing.role === 'tx' ? t('sensors.replaceTransmitter') : t('pair.waiting')}
      </p>
    {/if}
  </Modal>
{/if}
