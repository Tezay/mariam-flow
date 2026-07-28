# Architecture

Mariam Flow estimates, in real time, the waiting time of a queue (typically
at a university restaurant) using Wi-Fi sensing. Human bodies disturb the
multipath propagation of Wi-Fi radio waves; Channel State Information (CSI)
captures the amplitude and phase of the channel per OFDM subcarrier and
therefore encodes those disturbances. The system classifies the monitored
zone into four density classes and converts density into a waiting time with
Little's Law.

The system is device-free by design: no cameras, no personal data, no
interaction required from the people in the queue.

## System overview

```
[Queue zone]   ESP32-C6 TX ──(Wi-Fi traffic)──> ESP32-C6 RX ×2
                                   │ UDP (raw CSI frames)
                                   ▼
                       Edge aggregator (Linux)
                       ┌──────────────────────────────────────┐
                       │ flow-ingest   parse → ring buffer →  │
                       │               session storage        │
                       │ flow-infer    features → ONNX model  │
                       │               → density → wait time  │
                       │ flow-api      local REST + push      │
                       └──────────────────────────────────────┘
                                   │ HTTPS — aggregated estimates only:
                                   │ {wait, class, confidence, timestamp}
                                   ▼
                             Backend / display layer
```

**Privacy invariant: raw CSI never leaves the site.** Only aggregated
estimates (waiting time, density class, confidence, timestamp) are pushed
upstream.

## Components

| Path | Role | Status |
|---|---|---|
| `crates/flow-core` | Canonical domain types: CSI frames, density classes, labels, session metadata | Implemented |
| `crates/flow-ingest` | Frame parsing (esp-csi text format, see ADR 0005), stream reading with loss statistics, immutable on-disk session storage, `csi-replay` tool, UDP intake and the unified frame source | Implemented |
| `crates/flow-infer` | Window feature extraction (mirror of `flow_ml`), ONNX inference (`tract`), Little's Law, output smoothing, live pipeline and `csi-infer` tool | Full inference chain implemented; REST exposure planned |
| `crates/flow-api` | Local REST API (`axum`): live estimate, sessions, control; outbound push | Live-estimate surface and edge daemon implemented; sessions/control/push planned |
| `ml/` | Python package (`flow_ml`): session loading, feature engineering, training, ONNX export, visual reports | Loading, windowing, v1 features, training, grouped evaluation, ONNX export and reporting implemented |
| `firmware/csi-node` | C / ESP-IDF firmware for ESP32-C6 nodes, based on `espressif/esp-csi`; TX or RX role via sdkconfig; RX streams over serial (bring-up) or UDP (ADR 0007) | Serial capture validated on ESP32-C6; UDP path pending |
| `crates/flow-capture` | Labeled capture: session recording plus the phone labeling page (`csi-capture`) | Implemented |
| `crates/flow-edge` | The appliance daemon: validated configuration, installation lifecycle, administrator credential, authenticated HTTP surface, live estimation, event journal and estimate history, and the embedded dashboard | Configuration, lifecycle, credential, sessions, journal, live pipeline, history and dashboard serving implemented; pairing, network integration and calibration control planned |
| `dashboard/` | Svelte 5 single-page dashboard: sign-in, installation wizard shell, supervision; built to static assets and embedded in the daemon | Toolchain, brand tokens, translations, sign-in and shell implemented; wizard steps, calibration and live view planned |

The edge components are plain Rust binaries with no board-specific
dependency; any Linux/macOS machine can play the edge role during
development. The production cross-compilation target is
`aarch64-unknown-linux-gnu`.

## Data model

`flow-core` is the single source of truth for the data model. Its
serialization (serde/JSON) defines the canonical on-disk session format:

```
data/sessions/<session_id>/
├── meta.json        # SessionMeta: site, node placement, Wi-Fi channel,
│                    # firmware/software versions, environment description,
│                    # site-specific class mapping
├── csi.ndjson       # one CsiFrame per line:
│                    # {ts_us, node_id, rssi, mcs, len, amp[], phase[]}
└── labels.ndjson    # one Label per line: {ts_us, class, count?}
```

Format rules:

- Timestamps are microsecond-resolution Unix timestamps (`ts_us`), assigned
  by the edge at frame reception — node clocks are not trusted.
- One session is one continuous capture; recorded sessions are immutable.
  While a capture is running, the directory carries a `.recording` suffix;
  it is atomically renamed on finalization, so a truncated capture is
  always distinguishable from a clean one.
- `csi.ndjson` and `labels.ndjson` are append-ordered by `ts_us`; the
  writer rejects out-of-order timestamps, structurally invalid frames, and
  frames from nodes not declared in `meta.json`.
- `count` is the exact people count measured by a reference sensor during
  supervised calibration; when present, `class` is derived from it using the
  site-specific thresholds recorded in `meta.json`. Manually produced labels
  omit `count`. Storing the raw count keeps class boundaries re-derivable
  without recapturing sessions.
- Frames deserialized from a trust boundary (network, disk) must pass
  `CsiFrame::validate` before use; serde alone does not enforce structural
  invariants.
- `csi.ndjson` is the v0 human-readable format; migration to Parquet is
  planned once volumes require it.

### Node-to-edge frame format

Sensing nodes emit one `CSI_DATA` text line per measurement, in the format
of the stock `esp-csi` examples (ADR 0005). `flow-ingest` parses these
lines regardless of transport (serial capture, recorded file, UDP
datagram), supports both esp-csi column layouts (ESP32-C6 family and
classic ESP32, detected from the column count), and converts raw
interleaved I/Q values into per-sub-carrier amplitude and phase — a
bijective mapping, so nothing is lost. Edge rules are applied at
conversion: timestamps are assigned by the edge, and node MAC addresses
are mapped to logical `node_id`s from configuration.

Ingestion is resilient by policy: non-frame lines and malformed frames are
counted and skipped, never fatal to a capture. Frame loss is inferred from
gaps in per-transmitter sequence numbers and exposed as stream statistics,
which back the frame-loss quality metric of recorded sessions.

In production, nodes stream over UDP — one `CSI_DATA` line per datagram
(ADR 0007). The receiving node is identified by the datagram's source
address through an explicit mapping (`--node rx-1=192.168.4.11`,
repeatable); unknown senders are counted and dropped. Frames are stamped
by the edge clock at reception with a monotonic clamp, so the merged
multi-node stream is ordered by construction. The capture and inference
tools accept every transport through one input specification — a file,
`-` (stdin pipe), or `udp://ADDR:PORT`:

```sh
csi-capture --input udp://0.0.0.0:5566 \
            --node rx-1=192.168.4.11 --node rx-2=192.168.4.12 \
            --meta meta.json
flow-api    --input udp://0.0.0.0:5566 --node rx-1=192.168.4.11 \
            --node rx-2=192.168.4.12 --model model.onnx --config site.json
```

The `csi-replay` binary (in `flow-ingest`) turns any stream of `CSI_DATA`
lines — a recorded capture file, or stdin piped from a serial port — into a
canonical session directory: it filters frames by transmitter MAC (ambient
traffic exclusion), reconstructs monotonic edge timestamps from the node's
wrapping 32-bit local clock while preserving real inter-frame timing, and
writes through the session writer's invariant checks:

```sh
csi-replay --input capture.txt --meta meta.json --node-id rx-1 \
           --tx-mac aa:bb:cc:dd:ee:ff
cat /dev/ttyUSB0 | csi-replay --input - --meta meta.json --node-id rx-1
```

### Density classes

The classifier output space is frozen at four classes, encoded as integers:

| Value | Class | Meaning |
|---|---|---|
| 0 | `empty` | No detectable presence |
| 1 | `low` | Sparse presence, no meaningful queue |
| 2 | `medium` | Established queue, moderate density |
| 3 | `saturated` | Zone at or near capacity |

What each class concretely means at a given site (e.g. person counts in a
lab, queue landmarks in a restaurant) is recorded per session in the
`class_mapping` of `meta.json`.

The classifier's output is a probability distribution over the four classes,
not just the most likely class. The discrete class is the argmax; downstream
consumers (smoothing, wait-time estimation) operate on the full distribution,
which provides a continuous density signal at no extra cost. Classes are
discrete because the supervision signal is: ground truth comes either from a
human selecting one of four levels or from counts bucketed by documented
thresholds, and coarse ordered classes match the effective resolution of
indoor CSI, which saturates as zone density grows.

## ML-to-production bridge

Models are trained in Python (`ml/`) and exported to ONNX; the edge runs
inference in Rust through `tract`, so production carries no Python runtime.
Every exported model must pass a Python↔Rust parity test: identical inputs
must produce identical outputs within a 1e-5 tolerance.

The v1 exporter (`flow_ml.export`) hand-builds the ONNX graph from the
fitted pipeline using five core operators — `Sub, Div, MatMul, Add,
Softmax` — because tract does not register the `ai.onnx.ml` extension
operators that sklearn-specific converters emit (ADR 0006). The artifact
carries the whole pipeline, standardization included, so the edge cannot
mismatch the normalization. The parity contract is enforced in CI:
`flow_ml.export` writes fixtures (model plus sklearn-computed
probabilities) committed under `crates/flow-infer/tests/fixtures/`, and a
`flow-infer` integration test requires tract to reproduce them within
tolerance. `flow-infer` exposes the result as the full probability
distribution with derived class, confidence, and expected density level
(ADR 0002).

Evaluation uses cross-validation grouped by session — a single session is
never split between train and test, as adjacent windows of the same capture
are too similar and would leak.

### Feature extraction (v1)

`flow_ml` loads sessions with the same validations as the Rust side (the
two implementations are pinned by tests on the same canonical examples),
cuts them into time-based sliding windows (default 5 s, hop 1 s — time, not
frame counts, since the frame rate varies with losses), and computes
per-RX-node statistics per window: mean amplitude, temporal amplitude
deviation, motion energy (frame-to-frame change), mean inter-subcarrier
correlation, RSSI summary, and observed frame rate. Phase is not used in
v1: raw phase from unsynchronized commodity radios needs a dedicated
sanitization step first.

Ground truth is a step function over label timestamps; a window takes the
state at its center. Windows without ground truth, or incomplete for any
declared RX node, are dropped rather than imputed. The result is the
supervised dataset `(X, y)` consumed by training.

Live inference computes the same features in Rust (`flow-infer`), a
deliberate mirror of `flow_ml.features`: a silent divergence between the
two would skew every model output with no error anywhere. The mirror is
pinned by its own parity fixtures (windows of frames plus the
Python-computed reference vectors, 1e-8 tolerance on float64), generated
by `flow_ml.export` together with the ONNX fixtures. Reference vectors are
computed on float32-quantized inputs, since production frames always cross
the f32 `CsiFrame` representation.

### Training and evaluation (v1)

The v1 classifier is a multinomial logistic regression over standardized
features (a scikit-learn pipeline, so scaling parameters are learned on
training folds only). Its canonical output is the probability distribution
over the four classes (ADR 0002); the discrete class is the argmax.

Evaluation is session-grouped cross-validation: sessions are the split
unit, so every window is predicted exactly once by a model that never saw
its session — overlapping windows of one capture are heavily correlated
and would otherwise leak. Reports include accuracy against a
majority-class baseline and the full confusion matrix.

Deterministic synthetic sessions with separable classes
(`flow_ml.synthetic`) validate the pipeline end to end without hardware;
accuracy on them validates plumbing only, never field performance.

### Visual reports

`flow_ml.report` renders session portraits — one amplitude heatmap per RX
node over real frame timestamps (capture gaps stay visible), the
ground-truth band in the class palette, and the v1 features over time —
plus the evaluation figure (annotated confusion matrix with accuracy and
baseline). This is the first tool to run after any capture:

```sh
uv run python -m flow_ml.report --sessions data/sessions --out report/
uv run python -m flow_ml.report --demo 6 --out /tmp/report   # synthetic
```

## Wait-time estimation

Density is converted to a waiting time with Little's Law, W = L / λ,
implemented in `flow-infer` (`WaitEstimator`):

- each density class maps to an estimated number of people via a per-site
  calibrated mapping; the expected count E[L] is computed over the
  classifier's probability distribution, so the wait-time estimate evolves
  continuously instead of jumping at class transitions;
- λ is the service rate (people served per minute), calibrated per time
  slot; Little's Law assumes a stable regime, and the displayed range is
  what absorbs its degradation while the queue is still building up;
- smoothing is a time-aware exponential moving average — the continuous
  first-order low-pass `τ·ds/dt = x − s` discretized exactly for the
  elapsed time between samples (`α = 1 − e^(−Δt/τ)`), so the time constant
  holds regardless of sampling irregularity;
- the displayed class is stabilized by hysteresis (a Schmitt trigger on
  the smoothed 0–3 level): it only changes when the level crosses a class
  boundary by more than a configured margin, so boundary noise below the
  margin can never make the display flap;
- estimates carry the classifier confidence and a `reliable` flag; the
  publishing layer must not show unreliable estimates.

All parameters (class-to-people mapping, λ, τ, hysteresis margin,
confidence threshold) are per-site configuration, validated at
construction.

## Live inference

`flow-infer` ties the chain together in `LivePipeline`: frames are pushed
in stream order into a trailing time window (`(t − window, t]`,
duration-matched to the training windows — anchoring is statistically
irrelevant, duration is not), and an estimate is emitted every hop of
stream time once the first window has filled. Incomplete windows are
counted and skipped; after a capture gap, missed hops are never replayed.
At construction the pipeline cross-checks that the model's input width
equals `rx_nodes × features` — a configuration/model mismatch cannot start.

The `csi-infer` binary runs this chain on any stream of `CSI_DATA` lines
with a site configuration file (calibration parameters, window/hop):

```sh
csi-infer --input capture.txt --model model.onnx --config site.json
cat /dev/ttyUSB0 | csi-infer --input - --model model.onnx --config site.json --json
```

## Labeled capture

Supervised calibration runs through the `csi-capture` binary
(`flow-capture`), which combines session recording and ground-truth
labeling so both land in one session, stamped by one clock: the installer
labels from a phone over the LAN, and label timestamps are assigned by
the edge at HTTP reception — the phone's clock is never trusted, exactly
like the sensing nodes' clocks.

The labeling page is a single embedded vanilla-HTML file (no framework,
no build step) with four large color-coded buttons — each showing the
site-specific class description from the session's `class_mapping` — and
a status bar (recording state, frame count, duration, active label with a
live elapsed counter corrected for phone-vs-edge clock skew via the
server time exposed in `/status`). UI chrome is bilingual (English
default, French auto-detected) with a persisted 12/24-hour clock toggle;
class descriptions are site *content*, displayed verbatim — the
recommended convention is numeric ("6–15", "15+"), which reads in any
language. A wrong tap is corrected by tapping the right button: labels
form a step function, so a couple of mislabeled seconds are negligible
noise.

`Ctrl-C` or the end of the input stream flushes, syncs, and seals the
session. The tool serves no CSI data and is only run during calibration.

```sh
cat /dev/ttyUSB0 | csi-capture --input - --meta meta.json --node-id rx-1
# then open http://<edge-ip>:8088 on a phone
```

## Local REST API

The `flow-api` binary is the edge daemon: the blocking stream loop runs on
its own thread and publishes each estimate into a `watch` channel; the
async HTTP server (`axum`) reads the latest value. It binds to localhost
by default and never exposes raw CSI.

- `GET /health` — liveness probe.
- `GET /estimate` — the public contract. Only **reliable** (confidence
  above the site threshold) and **fresh** (younger than `--max-age-s`)
  estimates are exposed; anything else answers
  `{"status":"unavailable"}` without leaking values. Staleness masking
  means a dead stream degrades to "unavailable" on its own — a frozen
  wait time can never stay on display.
- `GET /internal/estimate` — operator view: the full internal state
  (people, level, reliability) plus whether and why the public endpoint
  masks it.

```sh
flow-api --input - --model model.onnx --config site.json \
         --listen 127.0.0.1:8080 --max-age-s 15
```

## Edge appliance

An installed site does not run the tools above by hand: it runs one
long-lived process, `flow-edge`, which embeds them as libraries and adds
what only a deployed unit needs — its configuration, its installation
lifecycle, and the dashboard an installer works from (ADR 0008). The
laboratory tools remain the fastest way to exercise one stage of the chain
in isolation and are unaffected.

### Network topology

The appliance hosts the access point the sensing nodes join, on a fixed
2.4 GHz channel matching the transmitter's — a station only senses CSI on
the channel it is associated with, so that channel cannot be allowed to
move. It therefore never doubles as a client of the site's network: the
built-in radio is dedicated to the sensor access point, and the site uplink
runs on a second interface, a USB Wi-Fi or USB Ethernet adapter (ADR 0009).
Both uplink kinds are configured through one code path.

The two networks are never bridged. Running with no uplink at all is a
supported mode rather than a failure: sensing, calibration and local
display work unchanged, and only remote supervision is unavailable. The
configuration consequently distinguishes an unanswered uplink question from
a deliberate choice to stay offline.

### Configuration and state

Configuration lives in one human-readable JSON file, validated on every
load and every save, and written atomically — a power cut during a write
leaves the previous configuration intact, and an invalid value is rejected
before it can reach the disk and lock the unit out of its next boot. It
carries the appliance identity, the sensor access point, the uplink, the
paired nodes and the per-site wait-estimation tuning; that tuning is a
field-for-field mirror of the `site.json` the laboratory tools read, so a
tuning produced in the lab moves into an appliance unchanged. Historical
series — node health, estimates, events — belong in SQLite instead, where
queries and retention are the natural operations.

Installation progress is *derived from facts* rather than stored as a
cursor: whether the site is named, the nodes are paired, the uplink
question is answered and a model is ready. An appliance interrupted
mid-installation resumes exactly where its configuration says it stands.
A single stored flag records that the installer closed the installation, so
a finished setup does not fall back into the wizard because a node is
temporarily unplugged.

### Stream arbitration

Calibration and live inference both consume the single UDP stream from the
receivers, so at most one may hold it. The daemon makes that a type-level
rule: every start requires an idle stream, and switching from one activity
to the other requires stopping first — an explicit act. A refused request
leaves the running activity untouched, so a stray "start live" cannot end a
calibration session an installer is halfway through.

### HTTP surface

The surface is **denied by default** (ADR 0011). Exactly two routes are
open: `GET /health`, a liveness probe that reveals nothing, and
`POST /api/session`, the login itself. Protected routes sit behind the
session guard as a group, so a route added there is protected by
construction rather than by remembering to protect it.

`GET /api/status` reports the appliance identity, installation progress,
current activity, sensor access point, uplink *shape* and paired nodes. Two
further rules bound what may appear there: no credentials — the uplink is
reported by mode and network name, never by passphrase, even though the
daemon holds it — and no raw CSI, the privacy invariant of the whole system.

### Sessions

A successful login exchanges the device secret for an opaque 256-bit token,
held in memory and delivered in an `HttpOnly`, `SameSite=Strict` cookie. The
token is tracked server-side so that logging out revokes it immediately.
Sessions expire on two independent clocks — 12 hours idle, 7 days absolute —
and never survive a reboot, so nothing bearer-shaped is written to the card.

Because verifying a secret costs an Argon2id hash, the login endpoint is
protected from being turned into either a guessing oracle or a way to
exhaust the appliance. Failures impose a doubling per-client delay (three
free attempts, then 1 s, 2 s, 4 s… to a five-minute ceiling, cleared on
success) rather than a lockout, which would let anyone on the site network
shut the installer out. Verification is also serialized process-wide and run
off the async runtime, so only one 19 MiB hash is ever in flight.

The secret travels in the request body, never in a query string. The QR code
on the label follows the same reasoning: it carries the secret in the URL
*fragment*, which browsers never transmit, so the dashboard reads it
client-side, exchanges it for a session and clears it from the address bar.

### Dashboard

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

The shell follows the appliance's phase rather than guessing: during
installation it is a full-frame wizard with no navigation, and the tabbed
shell appears only once the installation is closed. Translations are
dictionaries in the repository — English is the reference, other locales are
typed against it, so a missing key fails the build rather than the customer.

Embedding sits behind an optional Cargo feature, off by default: the crate
has to build, test and be developed on a machine with no Node installed, and
on a clean clone where nothing has been built. Without it the daemon serves a
notice saying so — a development state, never a deployment one.

```sh
cd dashboard && pnpm install && pnpm build
cargo build --release -p flow-edge --features dashboard
```

### Live estimation

The daemon runs the inference chain itself (ADR 0014). Frames are read on a
dedicated thread — a blocking loop that must not tie up the async runtime —
and each estimate is published through a `watch` channel, so the server never
waits on the pipeline and a slow browser cannot back-pressure sensing.

Starting requires a model, a tuned site and a receiver: exactly the facts the
installation readiness already tracks. An appliance missing one of them is not
reporting a fault, it is an installation that has not reached calibration; the
daemon names what is missing and serves regardless. The frame source is the
UDP socket the receivers stream to, or a recorded capture — which is how the
whole chain is exercised on a machine with no sensors attached:

```sh
flow-edge serve --config … --data-dir … \
                --input capture.txt --node-id rx-1 --tx-mac 1a:00:00:00:00:00
```

`GET /api/live` streams the state as server-sent events: one-way traffic, and
`EventSource` reconnects on its own when a phone's Wi-Fi drops. Each event
carries the estimate **and** the stream health, because a missing estimate
means one thing while the nodes are streaming and another once they have gone
silent. Per-node frame counts and rates are measured on stream time rather
than the wall clock, so a replayed capture reports the rate it was recorded
at.

### Estimate history

Estimates are folded into one row per minute and stored beside the event
journal. Keeping each one would cost roughly 1.9 GB a year against about
31 MB folded, and answers the same questions — a chart of a day paints
several minutes to a pixel.

The class of a minute is its most frequent class rather than an average:
density classes are ordinal labels, and the mean of `empty` and `saturated`
is not `medium`. Ties resolve towards the busier class, since understating a
queue costs more trust than overstating it. The mean level is kept alongside
for callers wanting a continuous curve, and reliability is a count rather
than a flag — a minute where two samples in sixty were trustworthy is not a
reliable minute. Retention is bounded on two axes, two years and a row
ceiling, exactly as the event journal is.

`GET /api/estimates?minutes=N` returns the most recent minutes in
chronological order, ready to plot.

### Journal

History lives in SQLite (`appliance.db`), not in files: it accumulates, it is
queried by time, and it has to be pruned (ADR 0012). One table records four
categories of event — access, appliance lifecycle, installation progress and
node connectivity — of which access and lifecycle have emitters today.

Durability is split by kind. Access events are committed before the call
returns, since those are the ones an attacker would erase by pulling the
power; everything else is buffered and written in one transaction every few
seconds, at shutdown, or when the buffer fills. The database runs in WAL mode
with `synchronous=FULL`, so an immediate write really has reached the card.
The daemon handles SIGTERM and SIGINT in order to flush rather than discard
what is buffered.

Access events carry the client address — administration data, never anything
about the people in the monitored queue. Because an address is personal data,
retention is bounded on two axes: a 90-day window, and a ceiling on retained
rows so a runaway loop cannot fill the card however recent its output. A
journal write that fails is reported and the request carries on; refusing to
authenticate anyone because the card filled up would be the worse failure.

`GET /api/events` reads it back, newest first, with a clamped `limit`.

### Administrator credential

Each unit carries its own secret, generated during preparation and printed
on its label: twenty characters over Crockford's base32 alphabet, grouped in
fours (`K7M4-9PQR-2WXY-6BTN-3HFD`), drawn uniformly from the operating
system's cryptographic generator — 100 bits (ADR 0010). The appliance stores
an Argon2id hash of it and never the secret itself. Verification normalizes
input first, so case, separators and the letters that resemble digits are
all forgiven; a secret read off a label under bad lighting still works.

Provisioning is a separate command run once per unit, and the daemon refuses
to serve without a credential — an appliance nobody can authenticate against
must not be reachable. Recovery is physical: the new secret is written into
a file on the card's boot partition, and the daemon consumes it at startup,
replaces the credential and deletes the file. A malformed recovery file is
reported and left in place while the previous credential stands, so a
mistyped secret cannot take a working installation offline.

```sh
flow-edge provision --data-dir /var/lib/mariam-flow   # once, during preparation
flow-edge new-secret                                  # print a secret, store nothing

flow-edge serve --config /etc/mariam-flow/appliance.json \
                --data-dir /var/lib/mariam-flow --listen 127.0.0.1:8080
```

## Toolchain and quality gates

- **Rust**: stable toolchain, edition 2024. CI enforces `cargo fmt --check`,
  `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and a
  cross-compilation check for `aarch64-unknown-linux-gnu`. Errors are typed
  with `thiserror`; `unwrap()`/`expect()` are confined to tests.
- **Python**: `uv`-managed environment (Python ≥ 3.12). CI enforces
  `ruff check`, `ruff format --check`, `pyright` (strict), and `pytest`.
- **Docs**: this file tracks the implemented state of the system; structural
  decisions are recorded as ADRs under `docs/adr/`.
