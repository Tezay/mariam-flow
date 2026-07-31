<script lang="ts">
  import type { Status } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';

  let { status }: { status: Status } = $props();
</script>

{#if status.nodes.length === 0}
  <p class="text-sm text-ink-500">{t('nodes.none')}</p>
{:else}
  <ul class="divide-y divide-ink-100">
    {#each status.nodes as node (node.node_id)}
      <li class="flex flex-wrap items-baseline justify-between gap-3 py-2">
        <span class="font-mono text-sm text-ink-900">{node.node_id}</span>
        <span class="text-sm text-ink-500">{t(`nodes.role.${node.role}` as const)}</span>
        <span class="font-mono text-xs text-ink-500">{node.address ?? node.mac ?? ''}</span>
      </li>
    {/each}
  </ul>
{/if}
