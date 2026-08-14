<script lang="ts">
  import Archive from '@lucide/svelte/icons/archive';
  import ChartNoAxesColumn from '@lucide/svelte/icons/chart-no-axes-column';
  import Download from '@lucide/svelte/icons/download';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import {
    type RecordedSession,
    archiveUrl,
    deleteSession,
    renameSession,
  } from '$lib/api/calibration';
  import { formatDay } from '$lib/calibration';
  import { page } from '$lib/paging';
  import { formattingLocale, hour12, t } from '$lib/i18n/i18n.svelte';
  import { formatClock } from '$lib/live';
  import Button from '$components/ui/Button.svelte';
  import ConfirmDialog from '$components/ui/ConfirmDialog.svelte';
  import NameDialog from '$components/ui/NameDialog.svelte';
  import Pager from '$components/ui/Pager.svelte';
  import SectionHeader from '$components/ui/SectionHeader.svelte';

  let {
    sessions,
    onchanged,
    onopen,
  }: {
    sessions: RecordedSession[];
    onchanged: () => void;
    onopen: (session: RecordedSession) => void;
  } = $props();

  let pageIndex = $state(0);
  let deleting = $state<RecordedSession | null>(null);
  let renaming = $state<RecordedSession | null>(null);

  const shown = $derived(page(sessions, pageIndex));

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
              <span class="block text-sm font-medium wrap-break-word text-ink-900">
                {session.environment || session.session_id}
              </span>
              <span class="block text-xs text-ink-500">
                {whenRecorded(session)} ·
                {t('cal.size', { value: (session.bytes / 1_048_576).toFixed(1) })}
              </span>
            </span>
            <span class="flex shrink-0 items-center gap-1">
              <Button variant="outline" size="sm" onclick={() => onopen(session)}>
                <ChartNoAxesColumn size={13} aria-hidden="true" />{t('recording.inspect')}
              </Button>
              {#if session.sealed}
                <Button
                  variant="quiet"
                  size="icon"
                  label={t('cal.export')}
                  title={t('cal.export')}
                  href={archiveUrl(session.session_id)}
                  download
                >
                  <Download size={14} aria-hidden="true" />
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
