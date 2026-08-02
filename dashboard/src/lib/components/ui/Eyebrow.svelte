<script lang="ts">
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();
</script>

<!-- The label above a group inside a panel. Small, quiet and uppercase: it
     names a region without competing with the rows under it. -->
<h3 class="text-[0.6875rem] font-semibold tracking-wider text-ink-500 uppercase">
  {@render children()}
</h3>
