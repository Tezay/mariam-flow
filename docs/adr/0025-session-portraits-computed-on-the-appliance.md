# Session portraits computed on the appliance

- Status: accepted
- Date: 2026-08-11
- Relates to: [0019](0019-calibration-recording-on-the-appliance.md),
  [0024](0024-the-training-run-reports-its-own-evaluation.md)

## Context and Problem Statement

A capture is recorded on site, exported, trained on elsewhere, and the model
comes back days later. Until then, nothing said whether the capture was worth
any of that.

The failures that make a capture useless are all invisible while it runs. A
receiver that never joined the network records nothing, and the appliance
reports it healthy because the other one is streaming. A node that goes quiet
for two minutes leaves a hole in the middle of the ground truth. A capture
nobody labelled records frames against no classes at all. Each of these is
obvious in hindsight and undetectable at the time.

Discovering any of them a week later, from a training run, costs the campaign:
the volunteers have gone, the furniture has moved, and the session cannot be
repeated under the same conditions.

ADR 0024 put the *model's* scores in the product. This is the other half — what
can be said about a capture before any model exists.

## Considered Options

- **Leave it to the training run.** Free, and answers days late.
- **Ship the Python report to the appliance.** The figures already exist —
  `flow_ml.report` renders an amplitude heatmap per node, the label band and
  the features over time. But a deployed unit carries no Python runtime, and
  the board has 512 MB; installing one to render a PNG is the tail wagging the
  dog.
- **Compute it in Rust on the appliance, once, and cache it.**

## Decision Outcome

**The appliance describes its own captures.**

A portrait is computed in one streaming pass over `csi.ndjson` and cached
beside the session as `portrait.json` plus `heatmap.bin`. It carries, per
receiver: frames and observed rate, the silences longer than a second, an
amplitude heatmap, and the seven v1 features over time — alongside the label
track and the span the capture covers.

**Nothing new is computed.** The features come from
`flow_infer::features::node_features`, the same function live inference uses,
pinned by parity fixtures against `flow_ml`. The appliance shows what a model
would be shown, not an approximation of it.

**Features use the training geometry (5 s window, 1 s hop), never the installed
model's.** A capture is described on its own terms: the same recording must not
read differently because a model was activated between two visits, and the
cache must not be invalidated by something that has nothing to do with it.

### Bounded by construction

Nothing is ever held whole. Frames arrive one at a time from a reader that
hands them out singly, and land in fixed-size accumulators:

- the heatmap accumulates **sums and counts per bin**, starting at 250 ms and
  **halving its resolution** whenever a capture proves long enough to exceed
  720 columns. Sums rather than means precisely so that merging two bins is an
  addition, with no weighting to get wrong;
- the features keep a sliding window of one analysis window per node.

Peak memory is a few megabytes whatever the capture's length. The cost is
parsing tens of megabytes of NDJSON — which is why the result is cached rather
than recomputed, and why the work runs on a blocking thread.

### Answering before the work is done

`GET /api/sessions/{id}/portrait` returns the description when it is cached,
and **`202` with `{"status": "computing"}`** otherwise, having started the work.
The client asks again. Holding the request open would time out on a long
capture with nothing to show for it.

Sealing a capture starts the same computation: whoever just stopped recording
is the one about to look, and the appliance is idle for exactly as long as they
take to get there.

### Two files, not one

The heatmap is one byte per `bin × subcarrier`, served raw from
`/portrait/heatmap`. Kept out of the JSON so the document stays inspectable
with `curl`, so the pixels stay the size they are — base64 would add a third —
and so the appliance never builds that string in memory. It is the same reason
session archives are streamed from a file rather than assembled.

**Zero is reserved for a bin no frame landed in.** Rendered on the amplitude
ramp a hole would read as the lowest amplitude, which is a measurement rather
than the absence of one.

A feature window too sparse to measure is `NaN`, which serialises as `null` —
again a hole rather than a value, and what training does with such a window
too.

### Encodings the reader must not re-invent

Two of them, both easy to get wrong in a way nothing would catch:

- a **density is an integer** in the portrait, `0..3`, because the portrait
  mirrors `labels.ndjson` rather than dressing it for display. Only the live
  estimate is given a name instead;
- the heatmap is **bin-major** — a bin's subcarriers sit together, the order it
  is accumulated in — while a canvas wants a row per subcarrier, so a reader
  transposes.

## Consequences

A capture can be judged the evening it is recorded, by looking at whether the
features move when the label track changes. That check needs no model, no
export and no training run.

A capture that never finished is described up to the break rather than
refused — it is the one most worth looking at, since something went wrong
during it.

The cache declares a schema and is recomputed rather than migrated when an
older build wrote it: a portrait can always be produced again from the session,
which cannot be said of anything else the appliance stores.

Deleting a session deletes its portrait with it, the two being one directory.
