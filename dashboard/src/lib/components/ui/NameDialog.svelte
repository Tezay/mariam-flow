<script lang="ts">
  import { untrack } from 'svelte';

  import { t } from '$lib/i18n/i18n.svelte';
  import Modal from '$components/ui/Modal.svelte';

  let {
    title,
    initial,
    onrename,
    oncancel,
  }: {
    title: string;
    initial: string;
    onrename: (name: string) => void;
    oncancel: () => void;
  } = $props();

  // Read once: the dialog is mounted per rename, so this is a draft from
  // here on rather than a mirror of the row behind it.
  let name = $state(untrack(() => initial));
</script>

<Modal {title} {oncancel}>
  <!-- Ahead of the button in the DOM so that a keyboard reaches the field
       first, and so that Enter submits rather than dismissing. -->
  <form
    onsubmit={(event) => {
      event.preventDefault();
      onrename(name.trim());
    }}
    class="contents"
  >
    <!-- svelte-ignore a11y_autofocus -->
    <input
      type="text"
      bind:value={name}
      autofocus
      required
      class="-mt-2 block w-full rounded-md border border-ink-200 px-3 py-2 text-base text-ink-900"
    />
    <button
      type="submit"
      disabled={name.trim().length === 0}
      class="rounded-md bg-mariam-600 px-4 py-3 text-sm font-medium text-white transition-colors
             hover:bg-mariam-700 disabled:bg-ink-200 disabled:text-ink-500"
    >
      {t('settings.save')}
    </button>
  </form>
</Modal>
