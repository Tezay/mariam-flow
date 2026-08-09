<script lang="ts">
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import type { Snippet } from 'svelte';

  let {
    title,
    open = false,
    children,
  }: {
    title: string;
    open?: boolean;
    children: Snippet;
  } = $props();
</script>

<details class="group rounded-md bg-white ring-1 ring-ink-200" {open}>
  <summary
    class="flex cursor-pointer items-center gap-2 px-4 py-3 text-sm font-medium text-ink-900
           transition-colors select-none hover:bg-ink-50
           list-none marker:content-none [&::-webkit-details-marker]:hidden"
  >
    <!-- `list-none` alone leaves the native triangle on Safari before 16.4,
         which the appliance is reached from often enough to matter. -->
    <ChevronRight
      size={14}
      class="shrink-0 text-ink-500 transition-transform group-open:rotate-90"
      aria-hidden="true"
    />
    {title}
  </summary>
  <div class="border-t border-ink-100">{@render children()}</div>
</details>
