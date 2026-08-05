# Dashboard

The installer works from a web dashboard the appliance serves: a Svelte 5
single-page application, prerendered to static assets and **compiled into the
daemon binary** (ADR 0013). The appliance therefore ships as one artifact,
with no way for dashboard and API to disagree about their version. Debug
builds read the same files from disk instead, so a frontend rebuild does not
mean recompiling Rust.

Everything is served from one origin: the daemon answers `/api` and
`/health`, and treats every other path as the dashboard's — an unmatched path
returns the application shell, which resolves it as a client-side route. The
session cookie is consequently a same-origin cookie, with no CORS anywhere.
Assets, the brand typeface included, are self-hosted without exception: an
appliance with no uplink is a supported mode, and the dashboard must render
identically there.

## Layout

```
dashboard/src/
  routes/                 the single page, and the layout that disables SSR
  lib/
    api/                  every call to the daemon, and the types it carries
    <domain>.ts           the rules a screen applies, each beside its tests
    paging.ts             list paging, shared by both histories
    i18n/                 the dictionaries; English is the reference
    tests/                setup and helpers for the component suites
    components/
      LoginScreen.svelte
      TabShell.svelte
      ui/                 primitives that know no domain
      wizard/             the installation wizard and its steps
      network/            uplink, survey and handout
      sensors/            where a failing installation is diagnosed
      calibration/        recordings, labeling, the model library
      live/               supervision and its history
      settings/           the sections, and the shell that resolves to one
```

A component's suffix says what kind of thing it is, so a file can be placed
without opening it:

| Suffix | What it is |
|---|---|
| `Screen` | fills the viewport; the page shows one at a time |
| `Panel` | the content of one tab |
| `Section` | one block within a panel or the settings |
| `Step` | one question of the wizard |
| `Shell` | a frame that chooses which of its children to show |

Two of those directories exist for a reason worth stating. `network/` is its
own domain rather than part of either neighbour because the wizard and the
settings configure the same thing — the site uplink is answered during
installation and revisited afterwards, from one set of components. And nothing
under `ui/` may import a domain module: a primitive that knows what a
calibration is stops being reusable, and the next screen copies it instead.

`lib/api/` carries one file per module of the daemon's HTTP surface — the
session, the installation, the sensing nodes, the service schedule,
calibration, models, the journal, the live stream — plus the status the whole
application revolves around and the transport the rest share. Changing a route
therefore means opening two files of the same name, one on each side. Every
type in there mirrors what `flow-edge` serializes, so a change on the Rust
side surfaces as a type error in the screens rather than as `undefined` at
runtime.

Tests sit beside what they test: `Foo.svelte` and `Foo.svelte.test.ts`. Suites
run under Node by default and opt into a DOM with a `@vitest-environment
jsdom` docblock, since the rule suites need no document and start faster
without one.

The screens themselves are described in
[dashboard-screens.md](dashboard-screens.md).
