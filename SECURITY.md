# Security policy

## Reporting a vulnerability

Please report privately rather than in a public issue, through GitHub's private
reporting on this repository: **Security → Report a vulnerability**
(<https://github.com/Tezay/mariam-flow/security>). Expect an acknowledgement
within a week.

Include what you did, what happened and what you expected — a request and its
response are usually enough. If the finding depends on a particular appliance
state (mid-installation, no model in service, outside service hours), say
which: several surfaces behave differently in each.

## Scope

The appliance daemon (`crates/flow-edge`) and the dashboard it serves:

- **The administrator credential** — a per-device secret verified with Argon2id,
  with a progressive per-client delay on failure.
- **The authenticated surface** — every route except `/health`,
  `POST /api/session` and `GET /estimate` requires a session cookie.
- **The public estimate** (`GET /estimate`) — answers without a credential and
  is the only route carrying a CORS header. It must never disclose a raw
  measurement, nor an operational detail that does not belong on a public
  screen.
- **Uploaded model bundles** — archives arrive from a browser and are treated
  as hostile: escaping paths refused, member sizes capped, and the bundle
  validated before anything is put into service.
- **The appliance journal** — the record of what happened, which must not be
  floodable into uselessness by an unauthenticated caller.
- **Session archives and exports** — identifiers that could escape their
  directory are refused, and operator-supplied names reach an HTTP header only
  as `[a-z0-9-]`.

## Out of scope

- **The sensing nodes.** ESP32 firmware derives from `espressif/esp-csi`;
  report firmware issues upstream.
- **The sensor access point.** A WPA2 network the nodes and the installer join.
  Anyone holding its passphrase is inside the trust boundary by design.
- **Radio denial of service.** Jamming the 2.4 GHz band stops the sensing;
  there is no defence at this layer and none is claimed.
- **Physical access.** Someone holding the card can read it. Credential
  recovery is deliberately a file on the boot partition, because recovering a
  unit means putting its card in a laptop.

## What the product promises

**Raw CSI never leaves the site.** Only aggregated estimates — waiting time,
density level, confidence, timestamp — are published. A finding that gets a
measurement out of the appliance is a serious one, whatever the route.

Nothing is collected about the people in the monitored queue: no camera, no
identifier, no per-person record anywhere in the system. The journal does keep
client addresses for access events, because a record of an attack that cannot
say where it came from is of little use. That is administration data, its
retention is bounded, and the network handout says so.

## Supported versions

The project is pre-1.0 and only `main` is supported; there are no released
versions to backport to.
