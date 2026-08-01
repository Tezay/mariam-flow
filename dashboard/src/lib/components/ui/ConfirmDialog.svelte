<script lang="ts">
  import Modal from '$components/ui/Modal.svelte';

  let {
    title,
    lead,
    confirmLabel,
    onconfirm,
    oncancel,
  }: {
    title: string;
    lead?: string;
    confirmLabel: string;
    onconfirm: () => void;
    oncancel: () => void;
  } = $props();
</script>

<Modal {title} {lead} {oncancel}>
  <button
    type="button"
    onclick={onconfirm}
    class="rounded-md bg-density-saturated px-4 py-3 text-sm font-medium text-white
           transition-colors hover:opacity-90"
  >
    {confirmLabel}
  </button>
</Modal>
