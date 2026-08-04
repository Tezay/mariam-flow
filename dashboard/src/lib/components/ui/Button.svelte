<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    variant = 'primary',
    size = 'md',
    type = 'button',
    href,
    download,
    disabled,
    onclick,
    label,
    title,
    children,
  }: {
    variant?: 'primary' | 'outline' | 'quiet' | 'danger';
    size?: 'md' | 'sm' | 'icon';
    type?: 'button' | 'submit';
    href?: string;
    download?: boolean;
    disabled?: boolean;
    onclick?: () => void;
    label?: string;
    title?: string;
    children: Snippet;
  } = $props();

  const VARIANTS = {
    primary:
      'bg-mariam-600 text-white hover:bg-mariam-700 disabled:bg-ink-200 disabled:text-ink-500',
    outline:
      'bg-white text-ink-900 ring-1 ring-ink-200 hover:bg-ink-50 disabled:text-ink-300 disabled:hover:bg-white',
    quiet: 'text-ink-500 hover:bg-ink-100 hover:text-ink-900 disabled:text-ink-300',
    danger: 'bg-density-saturated text-white hover:opacity-90 disabled:bg-ink-200',
  } as const;

  /* Comfortable under a thumb, compact under a pointer: the same appliance is
     operated from a phone in a service hall and from a laptop in an office. */
  const SIZES = {
    md: 'h-9 gap-2 rounded-md px-3 text-sm font-medium sm:h-8',
    sm: 'h-8 gap-1.5 rounded-md px-2.5 text-xs font-medium sm:h-7',
    icon: 'size-9 rounded-md sm:size-7',
  } as const;

  const shape = $derived(
    `inline-flex shrink-0 items-center justify-center transition-colors
     disabled:cursor-not-allowed ${SIZES[size]} ${VARIANTS[variant]}`,
  );
</script>

{#if href}
  <!-- These hrefs are appliance endpoints a browser downloads, not client
       routes, so the router must not resolve them. -->
  <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
  <a {href} {download} {title} aria-label={label} class={shape}>{@render children()}</a>
{:else}
  <button {type} {disabled} {onclick} {title} aria-label={label} class={shape}>
    {@render children()}
  </button>
{/if}
