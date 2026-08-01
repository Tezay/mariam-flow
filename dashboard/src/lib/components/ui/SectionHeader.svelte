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

<h2 class="flex items-center gap-2 text-sm font-semibold text-ink-900">
  {#if Icon}
    <Icon size={16} class="shrink-0 text-ink-500" />
  {/if}
  {title}
</h2>
{#if lead}
  <p class="mt-1 text-sm text-ink-500">{lead}</p>
{/if}
