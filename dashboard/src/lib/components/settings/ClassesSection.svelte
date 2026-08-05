<script lang="ts">
  import { untrack } from 'svelte';

  import { saveClasses } from '$lib/api/calibration';
  import { DENSITY_CLASSES } from '$lib/api/live';
  import { type ClassMapping, type Status } from '$lib/api/status';
  import { blankClasses } from '$lib/calibration';
  import { t } from '$lib/i18n/i18n.svelte';
  import Button from '$components/ui/Button.svelte';
  import Saved from '$components/ui/Saved.svelte';

  let { status, onupdated }: { status: Status; onupdated: (status: Status) => void } = $props();

  // Read once: a draft from here on.
  let classes = $state<ClassMapping>({ ...blankClasses(), ...untrack(() => status.classes) });
  let saving = $state(false);
  let saved = $state(false);

  const dirty = $derived(
    JSON.stringify(classes) !== JSON.stringify({ ...blankClasses(), ...status.classes }),
  );

  async function submit() {
    saving = true;
    const outcome = await saveClasses(classes);
    saving = false;
    if (outcome.kind === 'ok') {
      saved = true;
      setTimeout(() => (saved = false), 2500);
      onupdated(outcome.status);
    }
  }
</script>

<div class="max-w-md">
  <p class="text-sm text-ink-500">{t('cal.classesLead')}</p>

  <div class="mt-4 space-y-3">
    {#each DENSITY_CLASSES as density (density)}
      <label class="block text-sm text-ink-900">
        <span class="text-xs font-medium tracking-wide text-ink-500 uppercase">
          {t(`class.${density}` as const)}
        </span>
        <input
          type="text"
          bind:value={classes[density]}
          class="mt-1 block w-full rounded-md border border-ink-200 px-3 py-2 text-base
                 font-normal text-ink-900"
        />
      </label>
    {/each}
  </div>

  <div class="mt-3">
    <Saved shown={saved} message={t('cal.classesSaved')} />
    <Button disabled={saving || !dirty} onclick={() => void submit()}>
      {saving ? t('wizard.saving') : t('settings.save')}
    </Button>
  </div>
</div>
