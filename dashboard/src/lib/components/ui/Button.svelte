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
      'border border-mariam-600 text-mariam-600 hover:bg-mariam-50 disabled:border-ink-200 disabled:text-ink-300',
    quiet: 'text-ink-500 hover:bg-ink-100 hover:text-ink-900 disabled:text-ink-300',
    danger: 'bg-density-saturated text-white hover:opacity-90 disabled:bg-ink-200',
  } as const;

  const SIZES = {
    md: 'gap-2 rounded-md px-4 py-2 text-sm font-medium',
    sm: 'gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium',
    icon: 'rounded-md p-1.5',
  } as const;

  const shape = $derived(
    `inline-flex items-center justify-center transition-colors disabled:cursor-not-allowed
     ${SIZES[size]} ${VARIANTS[variant]}`,
  );
</script>

{#if href}
  <a {href} {download} {title} aria-label={label} class={shape}>{@render children()}</a>
{:else}
  <button {type} {disabled} {onclick} {title} aria-label={label} class={shape}>
    {@render children()}
  </button>
{/if}
