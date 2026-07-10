# Canonical capture-session format

- Status: accepted
- Date: 2026-07-10

## Context and Problem Statement

The labeled CSI dataset is the most valuable asset of the project: models,
evaluation, and any published dataset all derive from it. Every component
(firmware, ingestion, training, inference) must agree on one storage format
before the first capture, because re-capturing sessions is by far the most
expensive operation in the system.

Requirements: append-friendly during live capture, debuggable during early
development, self-describing (a session must be interpretable years later
without external context), immutable once recorded, and able to carry both
manual ground truth (a human picks a density class) and reference-sensor
ground truth (an exact people count).

## Considered Options

1. NDJSON files per session, migrating to Parquet when volume requires
2. Parquet from day one
3. A custom binary format
4. An embedded database (SQLite)

## Decision Outcome

Chosen option 1: one directory per session, three files, serialization
defined by the `flow-core` types:

```
data/sessions/<session_id>/
├── meta.json        # SessionMeta: site, node placement, Wi-Fi channel,
│                    # firmware/software versions, environment, class mapping
├── csi.ndjson       # one CsiFrame per line
└── labels.ndjson    # one Label per line: {ts_us, class, count?}
```

Format rules:

- Timestamps are microsecond-resolution Unix timestamps assigned by the edge
  at reception; node clocks are never trusted.
- One session is one continuous capture; recorded sessions are immutable.
- `count` is present when a reference sensor measured the exact number of
  people; `class` is then derived from the site-specific thresholds recorded
  in `meta.json`. Storing the raw count keeps class boundaries re-derivable
  from existing sessions, without recapture.
- The exact wire format (field names, integer class encoding) is frozen by
  unit tests in `flow-core`.

NDJSON is chosen for v0 because it is append-only by construction (one write
per line, crash-safe), streamable, human-readable, and trivially inspectable
with standard tools. Parquet (option 2) is superior for storage and columnar
reads but awkward to append during live capture and opaque during debugging;
it remains the planned migration target for archived sessions once volumes
require it. A custom binary format (option 3) adds parsing code and tooling
for no benefit at current data rates. SQLite (option 4) adds a dependency
and schema migrations while breaking the one-session-one-directory model
that makes sessions portable and publishable as plain files.

### Consequences

- Good: crash-safe appends; sessions are portable directories; format
  disagreements are caught by `flow-core` tests.
- Bad: NDJSON is verbose (roughly an order of magnitude larger than
  columnar storage) and slower to scan; acceptable at v0 rates, addressed
  by the planned Parquet migration for archives.
