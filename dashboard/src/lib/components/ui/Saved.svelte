<script lang="ts">
  import CircleCheck from '@lucide/svelte/icons/circle-check';

  let { shown, message }: { shown: boolean; message: string } = $props();
</script>

<!-- The height is held whether or not anything is shown: a confirmation that
     appears and disappears must not move the control that produced it. -->
<div class="min-h-5" aria-live="polite">
  {#if shown}
    <p class="flex items-center gap-1.5 text-xs text-ink-500">
      <CircleCheck size={14} class="text-success" aria-hidden="true" />{message}
    </p>
  {/if}
</div>
