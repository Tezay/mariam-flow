# Per-device secret as the administrator credential

- Status: accepted
- Date: 2026-07-28

## Context and Problem Statement

The appliance dashboard controls everything about an installation: pairing,
calibration, model activation, network configuration. Once the unit is
connected to the site network it is reachable by whoever else is on that
network — a campus LAN is not a trusted space. It therefore needs an
authentication scheme, and the scheme has to survive being operated by an
installer who is not a system administrator.

Three questions had to be settled together: who chooses the credential,
what the appliance stores, and how an installation recovers when the
credential is lost. The last one is not optional — a unit whose label has
been peeled off would otherwise be unusable hardware.

## Considered Options

1. A password chosen by the installer during the guided installation
2. A secret generated per unit, printed on its label, stored only as a hash
3. No credential on the sensor access point, one only for access from the
   site network

## Decision Outcome

Chosen option 2.

Option 1 fails on the only axis that matters for a credential: the strength
of an installer-chosen password is unknowable and, in practice, poor. It
also creates a bootstrap hole — the wizard that sets the password is itself
reachable before any password exists. Option 3 assumes the sensor access
point is a trusted space; it is not, because its passphrase is on the label
of every unit and known to anyone who has ever installed one.

**Format.** Twenty characters over Crockford's base32 alphabet — the digits
plus the uppercase letters, less `I`, `L`, `O` and `U` — displayed in groups
of four: `K7M4-9PQR-2WXY-6BTN-3HFD`. Dropping those four letters is what
makes the digits safe to keep: with no `O` there is nothing to confuse `0`
with. Exactly 32 symbols means exactly 5 bits per character, and
**100 bits** for the secret.

That figure describes the *generation process* — each character drawn
uniformly and independently from the operating system's cryptographic
generator — which is the only sense in which an entropy figure means
anything. The "charset entropy" reported by password strength meters
(length × log₂(alphabet)) describes the printed string instead, and would
score twenty identical characters just as highly.

**Storage.** The appliance stores an Argon2id hash in PHC string form and
never the secret. Parameters are the OWASP baseline (m = 19 MiB, t = 2,
p = 1); they are recorded inside the hash, so raising them later leaves
existing credentials verifiable. Memory-hardness is the point: it denies an
attacker holding the file the parallelism that makes commodity graphics
hardware effective against conventional hashes.

**Entry.** Verification normalizes first: case is ignored, separators may be
placed anywhere or omitted, and excluded letters fold onto the digit they
resemble (`I` and `L` to `1`, `O` to `0`). A secret read off a label under
bad lighting still works. Normalization costs nothing in strength — it maps
input onto the alphabet, it does not shrink the space the secret was drawn
from.

**Recovery** is physical, not remote: the new secret is written into a file
on the card's boot partition — chosen because it is the partition an
ordinary desktop can write to, so recovery means moving the card to a
laptop rather than having a login on the appliance. At startup the daemon
consumes that file, replaces the credential and deletes it, so the secret
does not linger in clear text. Possession of the card is the authorisation.
A malformed file is reported and left in place while the previous
credential stands: a mistyped recovery must not take a working installation
offline, and silently discarding the operator's request would leave them
locked out with no explanation.

Provisioning is a distinct command, run once while preparing a unit; it
refuses to overwrite an existing credential unless forced, because
re-provisioning a unit already in service invalidates the label on its case.

### Consequences

- Good: every unit ships with a credential of known, uniform strength, and
  no installation can be left with a weak or default one.
- Good: the appliance cannot disclose a secret it does not hold, and a
  stolen card yields a memory-hard hash rather than a password.
- Good: authentication is available from first boot, so the guided
  installation is protected rather than being the hole through which the
  appliance is configured.
- Bad: the credential is only as private as the label. Anyone with physical
  access to the case can read it — accepted, because physical access also
  permits the recovery procedure, and because the alternative is a
  credential the installer must be trusted to choose and keep.
- Bad: label and unit must stay together through preparation, shipping and
  installation, which is a logistics obligation rather than a technical
  one. Losing the pairing costs a card extraction.
- Bad: verification is deliberately expensive, so login must be throttled;
  an unthrottled endpoint would let an attacker consume the appliance's
  memory and CPU with parallel attempts. The throttle is a hard requirement
  of exposing this credential over HTTP, not an optimisation.
