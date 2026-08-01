<script lang="ts">
  import type { Snippet } from 'svelte';

  let { padded = true, children }: { padded?: boolean; children: Snippet } = $props();
</script>

<!-- The one surface of the application. Sections that need to divide it use
     `padded={false}` and pad their own rows, so a list still reads as one
     object rather than as a stack of cards. -->
<div class="overflow-hidden rounded-xl bg-white ring-1 ring-ink-100 {padded ? 'p-5' : ''}">
  {@render children()}
</div>
