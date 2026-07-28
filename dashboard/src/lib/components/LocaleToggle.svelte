<script lang="ts">
  import Languages from '@lucide/svelte/icons/languages';

  import { t, toggleLocale } from '$lib/i18n/i18n.svelte';
</script>

<button
  type="button"
  onclick={toggleLocale}
  class="inline-flex items-center gap-2 rounded-md px-3 py-2 text-sm text-ink-500
         transition-colors hover:bg-ink-100 hover:text-ink-900"
>
  <Languages size={16} aria-hidden="true" />
  {t('locale.switch')}
</button>
