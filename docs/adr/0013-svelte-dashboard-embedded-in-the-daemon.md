# Svelte single-page dashboard embedded in the daemon

- Status: accepted
- Date: 2026-07-29

## Context and Problem Statement

Plugging the appliance into mains power is the first step of an
installation; everything after it happens through a web dashboard the
appliance serves. That dashboard is the product surface — network setup,
node pairing, calibration, supervision — and it is operated by an installer
who is not a system administrator, from a phone, sometimes with no internet
at the site at all.

Three questions had to be answered together: what the dashboard is built
with, how it reaches the appliance, and how it is shipped.

## Considered Options

For the shipping: a second artifact served from a directory, or compiled
into the daemon binary.

For rendering: server-rendered pages from the Rust daemon, or a
client-rendered application against the existing JSON API.

For translations: a library, or dictionaries kept in the repository.

## Decision Outcome

**Svelte 5 with SvelteKit, prerendered to static assets, embedded in the
daemon binary.**

The appliance carries no Node runtime, so the application is compiled to
plain files with `adapter-static` and an `index.html` fallback: deep links
are resolved client-side. Rendering on the Rust side was rejected not on
taste but on duplication — the JSON API already exists and is already
authenticated, and templating the same state twice is where the two
inevitably drift.

The built files are **compiled into the binary** rather than laid beside it.
The appliance then ships as one artifact: nothing else to copy onto a card,
no way for dashboard and daemon to disagree about which version they are,
and no directory that can go missing on a unit nobody logs into. In debug
builds the files are read from disk instead, so a frontend rebuild does not
mean recompiling Rust — the development loop stays fast without the
deployment gaining a moving part.

Embedding is gated behind an optional Cargo feature, off by default. The
frontend is produced by a separate toolchain, and the crate must still
build, test and be worked on where that toolchain is absent — on a clean
clone, in the Rust CI job, on a contributor's machine without Node. A
release turns the feature on after the frontend build; without it the daemon
answers with a notice naming both commands, which is a development state and
never a deployment one. Committing a placeholder inside the output directory
was tried and rejected: the frontend build wipes that directory on every
run, taking the placeholder with it.

**Everything is served from one origin.** The daemon answers `/api` and
`/health` and treats every other path as the dashboard's. That keeps the
session cookie a same-origin cookie, with no CORS to configure and no
preflight to reason about; the development server proxies the same paths so
the browser sees the same shape it will see in production.

**Assets are self-hosted, without exception.** The brand typeface arrives as
an npm package and is bundled at build time. An appliance on a site with no
uplink — a supported mode, not a failure — must render identically, and a
dashboard that degrades when a font CDN is unreachable would fail exactly
when someone is standing in front of it trying to fix the network.

**Translations are dictionaries in the repository.** English is the
reference and every other locale is typed against it, so a missing key is a
compile error rather than a sentence that silently falls back in front of a
customer. A test covers the direction types cannot: keys left behind in a
translation after being removed from the reference, empty strings, and
placeholders that do not match. For two languages and a few hundred strings
this is a file and a test, against a dependency with its own build step and
conventions.

**The document declares its own content security policy.** Only the build
knows the hash of the start script SvelteKit inlines, so the policy travels
in the document rather than in a header: `script-src` is strict, everything
resolves to the appliance's own origin, and no remote host is reachable —
which the self-hosted assets already guaranteed. Inline styles are allowed,
because Svelte writes them when animating and blocking them buys far less
than blocking scripts does. What a document cannot express, the daemon sends
on every response: `nosniff`, `X-Frame-Options: DENY` (the header form of
`frame-ancestors`, which an administration interface should always refuse),
and `Referrer-Policy: no-referrer`, so the appliance's address never leaks
into a request the browser makes elsewhere.

**Distributed third-party assets carry their notices.** The typeface, the
icon set and the framework runtime are compiled into the binary under the
OFL, ISC and MIT licenses, all of which require their copyright notice to
travel with the software. `THIRD-PARTY-NOTICES.md` carries them, exactly as
`firmware/` already retains the Apache 2.0 notices inherited from esp-csi.

**The shell follows the appliance's phase.** During installation there is no
navigation at all: one step, full frame, the way a guided setup should feel.
Tabs appear only once the installation is closed — at the bottom of a phone
screen where a thumb reaches them, becoming a side rail when there is room.
Which shell to show is not a client-side guess: it comes from the phase the
daemon already computes from facts (ADR 0008), so an interrupted
installation resumes in the right shell after a reload.

### Consequences

- Good: one artifact to build, sign and copy; a version mismatch between
  dashboard and API is not expressible.
- Good: the dashboard works with no internet, which the offline mode
  requires.
- Good: no CORS, no second origin, no cookie configuration that differs
  between development and production.
- Bad: a release build now needs Node and pnpm to produce the embedded
  assets. Rust-only work is unaffected — the placeholder directory keeps
  `cargo build` and `cargo test` working without a JavaScript toolchain.
- Bad: a second language ecosystem in the repository, with its own lockfile,
  formatter and version drift to track. Mitigated by keeping the toolchain
  deliberately small, pinning it (`packageManager`, `engines`, `.nvmrc`) so
  CI and a developer's machine cannot diverge, and gating it in CI like
  every other language here.
- Bad: embedding third-party assets creates an attribution obligation that
  has to be maintained as dependencies change. The exhaustive per-crate
  listing for the Rust tree is still to be generated mechanically.
- Bad: updating the dashboard means shipping a new binary. Acceptable for an
  appliance updated as a unit, and it is what makes the single-artifact
  guarantee real.
