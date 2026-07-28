import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/**
 * The appliance serves these files itself, from the daemon binary. There is
 * no Node runtime on the unit, so everything is prerendered to static assets
 * with an `index.html` fallback: deep links resolve client-side.
 *
 * @type {import('@sveltejs/kit').Config}
 */
export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({ pages: 'build', assets: 'build', fallback: 'index.html' }),
    alias: { $components: 'src/lib/components' },

    /**
     * The content security policy is declared here rather than by the
     * daemon, because only the build knows the hash of the start script
     * SvelteKit inlines. Everything the dashboard needs is its own origin:
     * no CDN, no analytics, no remote font. `script-src` is strict, which
     * is the directive that actually stops an injected script from running.
     *
     * `style-src` allows inline styles: Svelte writes them when animating,
     * and blocking them buys far less than blocking scripts does.
     *
     * `frame-ancestors` is deliberately absent — a document-level policy
     * cannot carry it, so the daemon sends `X-Frame-Options` instead.
     */
    csp: {
      mode: 'hash',
      directives: {
        'default-src': ['self'],
        'script-src': ['self'],
        'style-src': ['self', 'unsafe-inline'],
        'img-src': ['self', 'data:'],
        'font-src': ['self'],
        'connect-src': ['self'],
        'object-src': ['none'],
        'base-uri': ['self'],
        'form-action': ['self'],
      },
    },
  },
};
