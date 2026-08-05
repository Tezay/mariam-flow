# Recording a labelled capture

A model is trained on captures where someone has said, in real time, how busy
the queue was. Two paths record them: a bench tool for bring-up, and the
appliance itself on an installed site.

## On the bench

`csi-capture` (`flow-capture`) combines session recording and ground-truth
labelling, so both land in one session stamped by one clock. The installer
labels from a phone over the LAN, and label timestamps are assigned by the edge
at HTTP reception — the phone's clock is not trusted, no more than a sensing
node's.

The labelling page is a single embedded HTML file with four large colour-coded
buttons, each showing the site-specific class description from the session's
`class_mapping`, and a status bar: recording state, frame count, duration, and
the active label with an elapsed counter corrected for phone-versus-edge clock
skew through the server time exposed in `/status`. Chrome is bilingual with a
persisted 12/24-hour toggle; class descriptions are site content, displayed
verbatim.

A wrong tap is corrected by tapping the right button — labels form a step
function, so a few mislabelled seconds are noise. `Ctrl-C` or the end of the
input stream flushes, syncs and seals the session. The tool serves no CSI data
and runs only during calibration.

```sh
cat /dev/ttyUSB0 | csi-capture --input - --meta meta.json --node-id rx-1
# then open http://<edge-ip>:8088 on a phone
```

## On the appliance

An installed site records its captures from the dashboard (ADR 0019).
Recording is another stage of the intake loop, exclusive of estimation by the
runtime's one-consumer rule, so estimation resumes on its own when a capture
ends: the estimator is skipped rather than torn down.

The session directory is created by the handler that starts the capture, so a
full card or a name already taken is answered to the caller rather than failing
on the intake thread; the frames are written by that thread. Labels are stamped
by the appliance clock, since the phone doing the labelling and the appliance
recording the frames are two machines.

A capture is refused while nothing is being read. Started with no stream, it
records labels against no frames, and whoever is labelling finds out an hour
later.

Session metadata is derived from the configuration wherever it can be — site,
paired nodes, radio channel, software version. Only what the appliance cannot
know is asked for: where each node sits, and what the density classes mean
here. Class meanings are settled once per site and copied into every session,
so the stored format stays readable on its own.

### What finishes an installation

A recorded session finishes the installation; the model does not. Recording
produces data, which is exported, trained off site and imported back days
later. Readiness therefore carries `site_captured`, which the wizard waits on,
separately from `model_ready`, which is reported and satisfied later from the
settings.

### Exporting

Recorded sessions are listed newest first, sorted on the identifier itself, so
no filesystem timestamp is consulted and the order is the same everywhere.

Each sealed session downloads as a gzipped tar for training elsewhere, built
into a temporary file and streamed from it — a capture runs to tens of
megabytes on a machine with 512 MB — and unlinked as soon as it is open. A
session identifier that could climb out of the sessions root is refused rather
than sanitized: it names a directory.

The archive filename carries the operator's name for the session, reduced to
`[a-z0-9-]` before it reaches the header, with the timestamp kept so two
captures of the same service cannot overwrite each other.
