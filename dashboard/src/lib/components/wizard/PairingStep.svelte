<script lang="ts">
  import Radio from '@lucide/svelte/icons/radio';
  import RadioTower from '@lucide/svelte/icons/radio-tower';

  import { type Discovery, fetchDiscovery, saveNodes } from '$lib/api/nodes';
  import { type Status } from '$lib/api/status';
  import { t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';
  import {
    EXPECTED_RECEIVERS,
    canConfirmPairing,
    pairingPayload,
    receiverShortfall,
  } from '$lib/wizard';

  let { onupdated }: { onupdated: (status: Status) => void } = $props();

  /** How often the offer is refreshed while the installer powers the nodes. */
  const POLL_MS = 2000;

  let discovery = $state<Discovery | null>(null);
  let saving = $state(false);
  let failure = $state<string | null>(null);

  $effect(() => {
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

  const proposal = $derived(discovery?.proposal ?? null);
  const shortfall = $derived(receiverShortfall(proposal));

  /** The rate of the candidate streaming from a proposed receiver's address. */
  function rateOf(address: string): number {
    return (
      discovery?.candidates.find((candidate) => candidate.address === address)
        ?.datagrams_per_second ?? 0
    );
  }

  async function confirm() {
    if (!proposal) {
      return;
    }
    saving = true;
    const outcome = await saveNodes(pairingPayload(proposal));
    saving = false;

    if (outcome.kind === 'ok') {
      failure = null;
      onupdated(outcome.status);
      return;
    }
    failure =
      outcome.kind === 'refused'
        ? t('wizard.refused', { message: outcome.message })
        : t('wizard.failed');
  }
</script>

<div class="max-w-md">
  {#if !discovery || discovery.candidates.length === 0}
    <p class="text-sm text-ink-500">{t('pair.waiting')}</p>
  {:else}
    <p class="text-xs font-medium tracking-wide text-ink-500 uppercase">
      {t('pair.heard', { count: discovery.candidates.length })}
    </p>

    <div class="mt-3 rounded-md bg-white p-4 ring-1 ring-ink-100">
      <h2 class="flex items-center gap-2 text-sm font-medium text-ink-900">
        <RadioTower size={16} aria-hidden="true" />{t('pair.transmitter')}
      </h2>
      {#if proposal?.tx_mac}
        <p class="mt-1 font-mono text-sm text-ink-900">{proposal.tx_mac}</p>
        <p class="text-xs text-ink-500">
          {t('pair.transmitterAgreed', { count: proposal.tx_agreement })}
        </p>
      {:else}
        <p class="mt-1 text-sm text-ink-500">{t('pair.transmitterMissing')}</p>
      {/if}
    </div>

    <div class="mt-3 rounded-md bg-white p-4 ring-1 ring-ink-100">
      <h2 class="flex items-center gap-2 text-sm font-medium text-ink-900">
        <Radio size={16} aria-hidden="true" />{t('pair.receivers')}
      </h2>
      <ul class="mt-2 divide-y divide-ink-100">
        {#each proposal?.receivers ?? [] as receiver (receiver.node_id)}
          <li class="flex items-baseline justify-between gap-3 py-2">
            <span class="font-mono text-sm text-ink-900">{receiver.node_id}</span>
            <span class="font-mono text-xs text-ink-500">{receiver.address}</span>
            <span class="text-sm tabular-nums text-ink-500">
              {t('pair.rate', { value: rateOf(receiver.address).toFixed(0) })}
            </span>
          </li>
        {/each}
      </ul>
    </div>

    {#if shortfall > 0}
      <!-- Said, never enforced: a second receiver may be installed later, and
           blocking here would also block repairing an installation that lost
           one. -->
      <p class="mt-3 text-sm text-density-medium">
        {t('pair.shortfall', {
          found: proposal?.receivers.length ?? 0,
          expected: EXPECTED_RECEIVERS,
        })}
      </p>
    {/if}
  {/if}

  {#if failure}
    <p role="status" class="mt-3 text-sm text-danger">{failure}</p>
  {/if}

  <div class="mt-4">
    <Button disabled={saving || !canConfirmPairing(proposal)} onclick={() => void confirm()}>
      {saving ? t('wizard.saving') : t('pair.confirm')}
    </Button>
  </div>
</div>
