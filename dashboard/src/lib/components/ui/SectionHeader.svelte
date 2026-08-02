<script lang="ts">
  import type { Component } from 'svelte';

  let {
    icon,
    title,
    lead,
  }: { icon?: Component<{ size?: number; class?: string }>; title: string; lead?: string } =
    $props();

  const Icon = $derived(icon);
</script>

<div class="mb-3">
  <h2 class="flex items-center gap-2 text-[0.9375rem] leading-6 font-semibold text-ink-900">
    {#if Icon}
      <Icon size={15} class="shrink-0 text-ink-500" />
    {/if}
    {title}
  </h2>
  {#if lead}
    <!-- One line, and short. A dashboard states what a panel is; it does not
         explain itself at paragraph length. -->
    <p class="mt-0.5 max-w-prose text-sm text-ink-500">{lead}</p>
  {/if}
</div>
