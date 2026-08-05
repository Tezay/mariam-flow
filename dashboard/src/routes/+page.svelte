<script lang="ts">
  import { fetchStatus, logout, type Status } from '$lib/api';
  import { t } from '$lib/i18n/i18n.svelte';
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
</script>

{#if screen.view === 'loading'}
  <main class="flex min-h-dvh items-center justify-center bg-ink-50 px-4">
    <p class="text-sm text-ink-500">{t('app.loading')}</p>
  </main>
{:else if screen.view === 'unreachable'}
  <main class="flex min-h-dvh flex-col items-center justify-center gap-4 bg-ink-50 px-4">
    <p class="text-sm text-ink-500">{t('app.unreachable')}</p>
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
{:else if screen.status.phase.phase === 'onboarding'}
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
