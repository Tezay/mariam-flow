# Models coming back

A model returns from training as an archive carrying `model.onnx`, the
`analysis.json` geometry it was trained under, and an optional `model.json`
manifest naming the run (ADR 0020). The geometry travels with the model: a
window a model never saw produces estimates that are plausible and wrong.

What the site turns a density into a waiting time with is not in the archive
and is never written by an import (ADR 0023).

## Arrival

Nothing is trusted on arrival. Members that could be written outside the
staging directory are refused, members that are not part of a bundle are
skipped, and sizes are capped against a decompression bomb.

Compatibility is decided by building the pipeline the bundle would run, rather
than by a check of its own — most often a model trained for a different number
of receivers. Only then are the files moved into the library, so a refused
import leaves nothing behind.

## The library

Every model is kept: a site recalibrated twice can return to the one that was
working, not only to the one immediately before.

Activation copies the chosen model and its geometry to the paths the pipeline
loads from, so the intake never has to know a library exists, and records the
handle in service in the configuration. The change is picked up through the
generation counter that already drives configuration reloads. Removing a model
removes the bundle alone; the recording it was trained on has its own lifetime.

The library is listed newest first, sorted on the handle, which encodes the
moment of import — the one date the appliance can vouch for. The training date
comes from the manifest, which it cannot.

## Names and handles

Renaming rewrites the name, never the handle. A model and a recording each
carry a dated directory name that other things point at: the configuration
records which model is in service, and metadata written at recording time
quotes the session identifier. The name a reader sees lives in a metadata file
inside the directory, so renaming rewrites that file and nothing else.

A bundle that arrived anonymous gains a manifest the first time it is named.
