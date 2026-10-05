<script lang="ts">
  import { logout } from '$lib/api/auth';
  import { type Status, fetchStatus } from '$lib/api/status';
  import {
    PROBE_EVERY_MS,
    connectionLost,
    lastHeardAt,
    watchConnection,
  } from '$lib/connection.svelte';
  import { t } from '$lib/i18n/i18n.svelte';
  import ConnectionNotice from '$components/ConnectionNotice.svelte';
  import LoginScreen from '$components/LoginScreen.svelte';
  import TabShell from '$components/TabShell.svelte';
  import WizardFrame from '$components/wizard/WizardFrame.svelte';

  type Screen =
    | { view: 'loading' }
    | { view: 'login' }
    | { view: 'ready'; status: Status }
    | { view: 'unreachable' };

  let screen = $state<Screen>({ view: 'loading' });

  // Which screen to show is decided by the appliance, not by the browser:
  // asking for the status answers both "is there a session" and "how far is
  // the installation", in one round trip.
  async function refresh() {
    const status = await fetchStatus();
    if (status === 'unauthorized') {
      screen = { view: 'login' };
    } else if (status === 'unreachable') {
      screen = { view: 'unreachable' };
    } else {
      screen = { view: 'ready', status };
    }
  }

  async function signOut() {
    await logout();
    screen = { view: 'login' };
  }

  void refresh();

  /* Narrowed to the view alone, so that a refreshed status does not restart
     the watch that delivered it. */
  const view = $derived(screen.view);

  $effect(() => {
    if (view === 'ready') {
      return watchConnection({
        onanswer: (status) => (screen = { view: 'ready', status }),
        onunauthorized: () => (screen = { view: 'login' }),
      });
    }
    if (view === 'unreachable') {
      const timer = setInterval(() => void refresh(), PROBE_EVERY_MS);
      return () => clearInterval(timer);
    }
  });

  const lost = $derived(connectionLost());
</script>

{#if screen.view === 'loading'}
  <main class="flex min-h-dvh items-center justify-center bg-ink-50 px-4">
    <p class="text-sm text-ink-500">{t('app.loading')}</p>
  </main>
{:else if screen.view === 'unreachable'}
  <main class="flex min-h-dvh flex-col items-center justify-center gap-4 bg-ink-50 px-4">
    <p class="text-sm text-ink-500">{t('app.unreachable')} {t('app.retrying')}</p>
    <button
      type="button"
      onclick={() => void refresh()}
      class="rounded-md bg-mariam-600 px-4 py-2 text-sm font-medium text-white
             transition-colors hover:bg-mariam-700"
    >
      {t('app.retry')}
    </button>
  </main>
{:else if screen.view === 'login'}
  <LoginScreen onauthenticated={() => void refresh()} />
{:else}
  {@const installing = screen.status.phase.phase === 'onboarding'}
  <!-- The tabbed shell scrolls inside a frame of the viewport's height; the
       installer scrolls the document, and only needs to fill it. -->
  <div class="app-shell flex flex-col {installing ? 'min-h-dvh' : 'h-dvh'}">
    {#if lost}
      <ConnectionNotice since={lastHeardAt()} />
    {/if}
    <!-- Inert as well as dimmed: an action sent to an appliance that is gone
         only fails later. -->
    <div class="flex min-h-0 flex-1 flex-col" class:opacity-60={lost} inert={lost}>
      {#if installing}
        <WizardFrame
          status={screen.status}
          onupdated={(next) => (screen = { view: 'ready', status: next })}
        />
      {:else}
        <TabShell
          status={screen.status}
          onsignout={() => void signOut()}
          onupdated={(next) => (screen = { view: 'ready', status: next })}
        />
      {/if}
    </div>
  </div>
{/if}
