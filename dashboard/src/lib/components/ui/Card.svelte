<script lang="ts">
  import type { Snippet } from 'svelte';

  let { padded = true, children }: { padded?: boolean; children: Snippet } = $props();
</script>

<!-- The one surface of the application. An 8 px radius and a border that is
     actually visible: a developer tool reads as a set of framed panels, not as
     a stack of soft tiles. Sections that divide it pad their own rows, so a
     list still reads as one object. -->
<div class="overflow-hidden rounded-md bg-white ring-1 ring-ink-200 {padded ? 'p-4 sm:p-5' : ''}">
  {@render children()}
</div>
