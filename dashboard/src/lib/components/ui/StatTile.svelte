<script lang="ts">
  let {
    label,
    value,
    hint,
  }: {
    label: string;
    value: string;
    hint?: string;
  } = $props();
</script>

<div class="rounded-md bg-white p-4 ring-1 ring-ink-200">
  <p class="text-xs font-medium tracking-wide text-ink-500 uppercase">{label}</p>
  <!-- Lining figures of one width, so a column of tiles does not shift as the
       numbers behind them change. -->
  <p class="mt-1 text-2xl font-semibold tabular-nums text-ink-900">{value}</p>
  {#if hint}
    <p class="mt-0.5 text-xs text-ink-500">{hint}</p>
  {/if}
</div>
