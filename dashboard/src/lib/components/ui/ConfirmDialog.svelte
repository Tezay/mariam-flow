<script lang="ts">
  import Button from '$components/ui/Button.svelte';
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
  <Button variant="danger" size="block" onclick={onconfirm}>{confirmLabel}</Button>
</Modal>
