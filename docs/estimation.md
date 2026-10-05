# From density to a waiting time

## Little's Law

Density is converted to a waiting time with `W = L / λ`, implemented in
`flow-infer` (`WaitEstimator`):

- each density class maps to an estimated number of people. The expected count
  `E[L]` is computed over the classifier's probability distribution, so the
  estimate moves continuously and does not jump at class transitions;
- `λ` is the service rate, in people served per minute;
- smoothing is a time-aware exponential moving average: the continuous
  first-order low-pass `τ·ds/dt = x − s`, discretized exactly for the elapsed
  time between samples (`α = 1 − e^(−Δt/τ)`), so the time constant holds
  whatever the sampling irregularity;
- the displayed class is stabilized by hysteresis, a Schmitt trigger on the
  smoothed 0–3 level. It changes only when the level crosses a class boundary
  by more than a configured margin;
- estimates carry the classifier confidence and a `reliable` flag.

The people counts, `λ`, `τ`, the hysteresis margin and the confidence threshold
belong to the site and are answered by the operator (ADR 0023). The analysis
window and hop belong to the model and travel with it. Every parameter is
validated at construction.

## The live pipeline

`flow-infer` ties the chain together in `LivePipeline`. Frames are pushed in
stream order into a trailing time window `(t − window, t]`, as long as the
training windows. An estimate is emitted every hop of stream time once the
first window has filled. Incomplete windows are counted and skipped, and after
a capture gap the missed hops are not replayed. At construction the pipeline
checks that the model's input width equals `rx_nodes × features`, so a
configuration and a model that disagree cannot start.

`csi-infer` runs this chain on any stream of `CSI_DATA` lines with a site
configuration file:

```sh
csi-infer --input capture.txt --model model.onnx --config site.json
cat /dev/ttyUSB0 | csi-infer --input - --model model.onnx \
                  --config site.json --json
```

## Estimating on the appliance

The daemon runs the same chain (ADR 0014). Frames are read on a dedicated
thread, a blocking loop that must not tie up the async runtime. Each estimate
is published through a `watch` channel, so the server does not wait on the
pipeline and a slow browser cannot back-pressure sensing.

Reading the stream and estimating from it are separate concerns (ADR 0017).
The intake opens its source whatever the installation stage. An estimator is
attached only when a model, a described queue and a receiver exist, and the
loop skips that stage when there is none. An appliance still being installed
therefore listens, which makes pairing from the stream possible.

`running` and `estimating` are reported separately. The absence of an estimator
is not an error, since readiness already says which step is outstanding. A
model that exists but cannot be loaded is a fault, and is reported.

The separation holds while the loop runs. A malformed or late frame says
nothing about the model and is skipped like any other unusable window. A model
that cannot run at all is detached and journalled. Either way the stream keeps
being read, so pairing, capture and sensor health survive a bad model, and
importing a replacement rebuilds the intake.

The intake is rebuilt whenever the configuration changes, since the sender
mapping and the site tuning are what the installation writes. Where a sensor
sits is the exception: the intake is built from nothing of it, and a position
given as a recording starts would otherwise rebuild it inside that recording.
The intake also ticks on a read timeout, so stream health and the senders it
has heard are refreshed without waiting for traffic. The frame source is the
UDP socket the receivers stream to, or a recorded capture:

```sh
flow-edge serve --config … --data-dir … \
                --input capture.txt --node-id rx-1 --tx-mac 1a:00:00:00:00:00
```

`GET /api/live` streams the state as server-sent events. Each event carries
the estimate, the stream health, whether a capture holds the stream, and the
appliance clock. The stream is driven by a one-second tick as well as by new
estimates: on estimates alone it would fall silent when every sensor has gone
quiet, or while a capture suspends estimation. The dashboard relies on that
tick to tell when the appliance has stopped answering
([dashboard-screens.md](dashboard-screens.md)). Per-node frame counts and
rates are measured on stream time, not the wall clock, so a replayed capture
reports the rate it was recorded at.

## Service hours

An appliance may declare when the site it watches is open (ADR 0015): an IANA
time zone, seven lists of intervals, and closure date ranges. Outside those
hours the pipeline stops estimating.

A closed hall has no queue, but the chain measures the channel and not people.
An empty room still yields a density class that smoothing carries for minutes,
and those minutes would enter the history as ordinary rows. Opening hours are
declared and not inferred, because a quiet signal cannot be told from a quiet
Tuesday.

Hours are resolved in the declared zone through the system time zone database.
An absent schedule means always open.

At each transition the minute in progress is flushed and a `service-opened` or
`service-closed` entry is journalled, so a flat afternoon can be told from an
outage. The stream keeps being read while closed, so sensor health stays
observable out of hours.

`PUT /api/service-window` replaces the whole schedule, or clears it with
`null`. It is not patched field by field: intervals of a day must not overlap
and a closure must not end before it begins, and validating one field against
a stored remainder would check only half of that. A refusal names the day at
fault and leaves the stored schedule untouched.

## The public estimate

`GET /estimate` is the one route that answers without a session. It is served
by `flow-edge` and publishes the aggregate only.

Three states:

```
GET /estimate                       (no session, no cookie)

{ "status": "ok", "wait_min": 6.5, "class": "medium",
  "confidence": 0.81, "ts_us": 1785713000000000 }

{ "status": "closed", "opens_at_us": 1785740400000000 }

{ "status": "unavailable" }
```

An estimate is published only when it is reliable, with a confidence above the
site threshold, and less than ninety seconds old. A closed site is reported as
closed and not as a fault.

The refusal carries no reason. This is the surface a hall display reads, and
why the appliance cannot estimate is answered by the dashboard and the journal.
The route answers with `Access-Control-Allow-Origin: *`, and no other route on
the appliance carries a CORS header.
