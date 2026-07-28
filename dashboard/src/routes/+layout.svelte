<script lang="ts">
  import '../app.css';
  import { initLocale } from '$lib/i18n/i18n.svelte';

  let { children } = $props();

  initLocale();
</script>

{@render children()}
