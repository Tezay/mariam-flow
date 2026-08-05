import { createRawSnippet } from 'svelte';

/** A snippet rendering `value`, which a `.ts` file cannot express directly. */
export function text(value: string) {
  return createRawSnippet(() => ({ render: () => `<span>${value}</span>` }));
}
