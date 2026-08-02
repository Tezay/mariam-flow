<script lang="ts">
  import Archive from '@lucide/svelte/icons/archive';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Circle from '@lucide/svelte/icons/circle';
  import Download from '@lucide/svelte/icons/download';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import {
    archiveUrl,
    deleteSession,
    renameSession,
    startCalibration,
    type RecordedSession,
    type SensingNode,
    type Status,
  } from '$lib/api';
  import { formatDay, page } from '$lib/calibration';
  import { formattingLocale, hour12, t } from '$lib/i18n/i18n.svelte';
  import { formatClock } from '$lib/live';
  import Button from '$components/ui/Button.svelte';
  import ConfirmDialog from '$components/ui/ConfirmDialog.svelte';
  import NameDialog from '$components/ui/NameDialog.svelte';
  import Pager from '$components/ui/Pager.svelte';
  import SectionHeader from '$components/ui/SectionHeader.svelte';

  let {
    nodes,
    sessions,
    ready,
    streaming,
    environment = $bindable(),
    positions = $bindable(),
    onupdated,
    onchanged,
  }: {
    nodes: SensingNode[];
    sessions: RecordedSession[];
    ready: boolean;
    streaming: boolean;
    environment: string;
    positions: Record<string, string>;
    onupdated: (status: Status) => void;
    onchanged: () => void;
  } = $props();

  let pageIndex = $state(0);
  let deleting = $state<RecordedSession | null>(null);
  let renaming = $state<RecordedSession | null>(null);
  let busy = $state(false);
  let failure = $state<string | null>(null);

  const shown = $derived(page(sessions, pageIndex));

  async function start() {
    busy = true;
    const outcome = await startCalibration({ environment, positions });
    busy = false;
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

  async function rename(name: string) {
    if (renaming && (await renameSession(renaming.session_id, name))) {
      onchanged();
    }
    renaming = null;
  }

  async function remove() {
    if (deleting && (await deleteSession(deleting.session_id))) {
      onchanged();
    }
    deleting = null;
  }

  function whenRecorded(session: RecordedSession): string {
    if (session.recorded_at_us === undefined) {
      return session.session_id;
    }
    const locale = formattingLocale();
    return `${formatDay(session.recorded_at_us, locale)} ${formatClock(session.recorded_at_us, locale, hour12())}`;
  }
</script>

<section>
  <SectionHeader icon={Archive} title={t('prepare.title')} lead={t('prepare.lead')} />

  <div class="mt-4 overflow-hidden rounded-md bg-white ring-1 ring-ink-200">
    <div class="p-5">
      <label class="block text-sm font-medium text-ink-900">
        {t('cal.environment')}
        <input
          type="text"
          bind:value={environment}
          placeholder={t('cal.environmentHint')}
          class="mt-1 block w-full rounded-md border border-ink-200 px-3 py-2 text-base
                 font-normal text-ink-900"
        />
      </label>

      <details class="group mt-4">
        <summary
          class="flex cursor-pointer items-center gap-1 text-sm text-ink-500 hover:text-ink-900"
        >
          <ChevronDown
            size={14}
            class="transition-transform group-open:rotate-180"
            aria-hidden="true"
          />
          {t('cal.positions')}
        </summary>
        <div class="mt-2 ml-5 space-y-2">
          {#each nodes as node (node.node_id)}
            <label class="block text-sm text-ink-900">
              <span class="font-mono text-xs text-ink-500">{node.node_id}</span>
              <input
                type="text"
                bind:value={positions[node.node_id]}
                class="mt-1 block w-full rounded-md border border-ink-200 px-3 py-2 text-base
                       font-normal text-ink-900"
              />
            </label>
          {/each}
        </div>
      </details>

      {#if failure}
        <p role="status" class="mt-3 text-sm text-danger">{failure}</p>
      {/if}
      {#if !ready}
        <p class="mt-3 text-sm text-ink-500">{t('cal.notReady')}</p>
      {:else if !streaming}
        <p class="mt-3 text-sm text-density-medium">{t('cal.noStream')}</p>
      {/if}

      <div class="mt-4">
        <Button
          variant="danger"
          disabled={busy || !ready || !streaming || environment.trim().length === 0}
          onclick={() => void start()}
        >
          <Circle size={14} class="fill-current" aria-hidden="true" />
          {busy ? t('cal.starting') : t('cal.start')}
        </Button>
      </div>
    </div>

    <h3
      class="border-t border-ink-100 px-5 py-3 text-xs font-medium tracking-wide text-ink-500
             uppercase"
    >
      {t('cal.history')}
    </h3>
    {#if sessions.length === 0}
      <p class="px-5 pb-5 text-sm text-ink-500">{t('cal.historyEmpty')}</p>
    {:else}
      <ul class="divide-y divide-ink-100 border-t border-ink-100">
        {#each shown as session (session.session_id)}
          <li class="flex flex-wrap items-center justify-between gap-3 px-5 py-3">
            <span class="min-w-0 flex-1">
              <span class="block text-sm font-medium break-words text-ink-900">
                {session.environment || session.session_id}
              </span>
              <span class="block text-xs text-ink-500">
                {whenRecorded(session)} ·
                {t('cal.size', { value: (session.bytes / 1_048_576).toFixed(1) })}
              </span>
            </span>
            <span class="flex shrink-0 items-center gap-1">
              {#if session.sealed}
                <Button variant="outline" size="sm" href={archiveUrl(session.session_id)} download>
                  <Download size={13} aria-hidden="true" />{t('cal.export')}
                </Button>
              {:else}
                <span class="px-2 text-xs text-density-medium">{t('cal.unfinished')}</span>
              {/if}
              <Button
                variant="quiet"
                size="icon"
                label={t('cal.rename')}
                title={t('cal.rename')}
                onclick={() => (renaming = session)}
              >
                <Pencil size={14} aria-hidden="true" />
              </Button>
              <Button
                variant="quiet"
                size="icon"
                label={t('cal.delete')}
                title={t('cal.delete')}
                onclick={() => (deleting = session)}
              >
                <Trash2 size={14} aria-hidden="true" />
              </Button>
            </span>
          </li>
        {/each}
      </ul>
      <Pager total={sessions.length} bind:index={pageIndex} />
    {/if}
  </div>
</section>

{#if renaming}
  <NameDialog
    title={t('cal.rename')}
    initial={renaming.environment}
    onrename={(name) => void rename(name)}
    oncancel={() => (renaming = null)}
  />
{/if}

{#if deleting}
  <ConfirmDialog
    title={t('cal.confirmDelete')}
    lead={t('cal.confirmDeleteLead')}
    confirmLabel={t('cal.delete')}
    onconfirm={() => void remove()}
    oncancel={() => (deleting = null)}
  />
{/if}
