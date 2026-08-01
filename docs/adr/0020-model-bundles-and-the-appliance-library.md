# Model bundles and the appliance library

- Status: accepted
- Date: 2026-08-01

## Context and Problem Statement

A recorded calibration session is exported, trained on elsewhere, and the
result comes back to the appliance days later (ADR 0019). That return trip is
the last link of the loop and the only one performed by someone holding a file
they did not produce on a machine they cannot inspect.

Two questions follow. What exactly comes back — a model, or a model and the
settings it was trained under? And what happens to the model the appliance was
already running?

## Considered Options

For the payload: a bare `model.onnx`, a bare model plus settings typed into the
dashboard, or an archive carrying both.

For the previous model: replace it, keep one predecessor for rollback, or keep
every model the appliance has been given.

## Decision Outcome

**A model travels with the tuning it was trained under, in one archive.** The
window and hop a model saw during training are part of what it means; feeding
it a window it never saw produces estimates that are plausible and wrong, with
no symptom to notice. Asking an operator to type those numbers alongside the
file makes a silent mistake possible, so the bundle carries `model.onnx`,
`site.json`, and an optional `model.json` naming the training run.

The manifest is optional on purpose. A bundle without one is still usable —
it is anonymous, and named after the moment it arrived — because refusing it
would strand a model that works over metadata that only helps a human read a
list.

**Every model is kept.** A site recalibrated twice should be able to return to
the model that was working, not only to the one immediately before, and a model
is kilobytes. Import therefore files the bundle under a dated handle and
activates it; the models already held stay where they are.

Activation **copies** the model to the single path the pipeline loads from
rather than pointing at the library. The intake then never has to know a library
exists, and the configuration carries only which handle is in service — a fact
that survives being read by something that has never heard of bundles.

## Nothing is trusted on arrival

The archive arrives from a browser, and an authenticated caller is not a
trusted one. Three rules apply before anything is written:

- **The path rule is about paths, not names.** A member that could be written
  anywhere but the staging directory is refused outright. A member that is
  merely not one of the three — a README, or the metadata a desktop archiver
  slips in — is skipped: it is harmless, and refusing it turns an ordinary
  archive into a puzzle.
- **Members are capped** at a size no real model approaches, against a
  decompression bomb rather than against a model.
- **Compatibility is decided by building the pipeline the bundle would run**,
  not by a check of its own. The pipeline already knows what it requires — the
  receiver count above all — and a second opinion is one more thing to keep in
  step with it.

A bundle that fails any of these is refused with the reason, and the appliance
keeps running what it was running.

## Consequences

Staging happens in a temporary directory and the files are moved into the
library only once the bundle has been read, parsed, and proved to drive this
appliance. A refused import therefore leaves nothing behind.

The active tuning is taken from the bundle and written into the configuration,
which bumps the generation counter the intake supervisor watches — so the new
model is picked up without a restart, by the mechanism that already existed for
configuration changes.

Removing a model removes only the bundle. The recording it was trained on is a
separate object with a separate lifetime, and the appliance says so where the
removal is confirmed.

The library is listed newest first, sorted on the handle itself, so no
filesystem timestamp is consulted and the order is the same everywhere. The
handle encodes the moment of import, which is the one date the appliance can
vouch for; the manifest supplies the training date, which it cannot.
