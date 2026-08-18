# Models coming back

A model returns from training as an archive carrying `model.onnx`, the
`analysis.json` geometry it was trained under, the `evaluation.json` scores it
earned, and an optional `model.json` manifest naming the run (ADR 0020). The
geometry travels with the model: a window a model never saw produces estimates
that are plausible and wrong.

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

Loading a model checks both ends of it: the input width against the receivers
this appliance has, and the output against the four classes. A run that never
observed one of them exports a graph one column short, which the input check
alone would accept and the first estimate would then fail on — after the model
had been put in service.

## Reading how a model scored

A model reports what its training run measured, which is the only evidence the
appliance has that one model is better than another. `GET /api/models/{id}`
answers one model with its evaluation; the listing carries only whether there
is one, since a library is read on every visit to the calibration screen and a
matrix is read once someone has chosen a model.

What the screen shows is derived from the confusion matrix rather than copied
out of it, because a single accuracy figure says whether a model is right and
never how it is wrong:

- whether an empty zone is told from an occupied one, the three occupied
  classes merged — the floor, below which nothing else can be trusted;
- what share of the mistakes land on a neighbouring class, since the classes
  are ordered and hesitating on a boundary is not the same fault as answering
  at random;
- whether the exact level is found, alongside the majority-class baseline it
  has to beat.

The captures a model was trained on are named, and resolved against the
recordings the appliance still holds: a capture exported and removed is
reported as gone rather than silently omitted.

Two models are read against each other the same way, every difference stated on
the candidate, and level by level as well as in aggregate — a rare level lost
is invisible in an accuracy that improved. Whether a difference means anything
depends on what each was measured against, so two runs held out on different
recordings are named as such: they answer different questions, and an arrow on
its own would imply a ranking neither earned.

A bundle that arrived before training runs reported their scores says so, and
estimates normally.

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
