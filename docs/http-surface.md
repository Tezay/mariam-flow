# The HTTP surface

## Access

The surface is denied by default (ADR 0011). Three routes are open:
`GET /health`, a liveness probe that reveals nothing; `POST /api/session`, the
login; and `GET /estimate`, the public estimate. Every other route sits behind
the session guard as a group, so a route added there is protected by
construction.

Handlers are grouped by subject — the session, the installation, the sensing
nodes, the service schedule, calibration, models, the journal, the live stream
— each in its own module beside the tests that exercise it through the router.
The router is the one place that says which of them is protected.

Two rules bound what may cross this boundary: no credentials, the uplink being
reported by mode and network name and never by passphrase; and no raw CSI, the
privacy invariant of the whole system.

## Writes

The candidate configuration is validated before the save that would validate it
anyway, because only that error names the field at fault. The in-memory copy is
replaced once the write has succeeded. `PUT /api/site`, `/api/nodes`,
`/api/uplink`, `/api/queue`, `/api/installation` and `/api/service-window` all
go through it.

Closing the installation is refused while any step is outstanding, and the
refusal names that step. Reopening is always allowed.

`GET /api/status` reports the appliance identity, installation progress,
current activity, sensor access point, uplink shape, queue description and
paired nodes.

## Answers that outlast a request

Describing a capture means parsing a file measured in tens of megabytes.
`GET /api/sessions/{id}/portrait` therefore answers `202` with
`{"status": "computing"}` and starts the work on a blocking thread, or `200`
with the description once it is cached; the caller asks again. A capture being
described is recorded so a reader who polls does not start the work twice.

Its heatmap is served separately, as raw bytes from
`/api/sessions/{id}/portrait/heatmap`: it is the bulk of a description, the
browser hands it straight to a canvas, and keeping it out of the JSON leaves
that document readable with `curl`.

## Sessions

A successful login exchanges the device secret for an opaque 256-bit token,
held in memory and delivered in an `HttpOnly`, `SameSite=Strict` cookie. The
token is tracked server-side so logging out revokes it immediately. Sessions
expire on two independent clocks — 12 hours idle, 7 days absolute — and never
survive a reboot, so nothing bearer-shaped is written to the card.

Verifying a secret costs an Argon2id hash, so the login endpoint is protected
from being turned into a guessing oracle or a way to exhaust the appliance.
Failures impose a doubling per-client delay — three free attempts, then 1 s,
2 s, 4 s, to a five-minute ceiling, cleared on success — rather than a lockout,
which would let anyone on the site network shut the installer out. Verification
is serialized process-wide and run off the async runtime, so only one 19 MiB
hash is ever in flight.

The secret travels in the request body, never in a query string. The QR code on
the label carries it in the URL fragment, which browsers do not transmit: the
dashboard reads it client-side, exchanges it for a session and clears it from
the address bar.

## Administrator credential

Each unit carries its own secret, generated during preparation and printed on
its label: twenty characters over Crockford's base32 alphabet, grouped in fours
(`K7M4-9PQR-2WXY-6BTN-3HFD`), drawn from the operating system's cryptographic
generator — 100 bits (ADR 0010). The appliance stores an Argon2id hash and
never the secret. Verification normalizes input first, so case, separators and
the letters that resemble digits are forgiven.

Provisioning is a separate command run once per unit, and the daemon refuses to
serve without a credential. Recovery is physical: the new secret is written
into a file on the card's boot partition, and the daemon consumes it at
startup, replaces the credential and deletes the file. A malformed recovery
file is reported and left in place while the previous credential stands.

```sh
flow-edge provision --data-dir /var/lib/mariam-flow \
                    --config /etc/mariam-flow/appliance.json \
                    --kit-id KIT-0001 --ap-ssid mariam-flow-0001 \
                    --ap-passphrase …
flow-edge new-secret                # print a secret, store nothing

flow-edge serve --config /etc/mariam-flow/appliance.json \
                --data-dir /var/lib/mariam-flow --listen 127.0.0.1:8080
```
