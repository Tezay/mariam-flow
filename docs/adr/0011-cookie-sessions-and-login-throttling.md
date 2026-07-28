# Cookie sessions, login throttling, and a deny-by-default surface

- Status: accepted
- Date: 2026-07-28

## Context and Problem Statement

ADR 0010 gave each appliance a device secret and stored it as an Argon2id
hash. Presenting that secret on every request is not an option: the hash is
deliberately expensive, so it must be paid once and exchanged for something
cheap to check. That raises three questions at once — what the something is
and how long it lasts, how the exchange is protected from being attacked,
and which routes require it.

The last question is not incidental. The dashboard's surface will grow to
cover pairing, calibration, model activation and network configuration, and
most of that will be written months from now. Whichever way the default
falls is the way a forgotten route will fall.

## Considered Options

For the session token: an opaque value held server-side, or a signed
self-contained token (JWT-style) with no server state.

For the exchange: no protection beyond the hash cost, a hard lockout after
N failures, or a per-client delay that grows.

For the surface: allow by default with protected routes marked, or deny by
default with open routes marked.

## Decision Outcome

**An opaque token, held in memory, delivered in a cookie.** 256 bits from
the system's cryptographic generator, looked up in a map. A signed
self-contained token would spare the server that map, but it buys
statelessness the appliance has no use for — there is one process, and it
is not scaled out — and pays for it with the property that matters most
here: revocation. Logging out, or ending every session at once, has to be
immediate, and a token the server does not track cannot be withdrawn before
it expires on its own.

Sessions are held in memory only, so a reboot ends all of them and nothing
bearer-shaped is ever written to the card. Two independent bounds apply,
because they answer different questions: an **idle timeout** of 12 hours,
which expires a phone put down and forgotten, and an **absolute lifetime**
of 7 days, which stops a stolen token from being kept alive indefinitely by
an attacker who simply keeps using it. The store is capped, since an
appliance serves a handful of people and an unbounded map is a liability on
a 512 MB machine.

The cookie is `HttpOnly`, so a cross-site scripting flaw in the dashboard
cannot read the token, and `SameSite=Strict`, so another site cannot ride
the session with a forged request. `Secure` is deliberately absent: on the
sensor access point the dashboard is served over plain HTTP — the captive
portal requires it — and a `Secure` cookie would never be sent there. That
traffic is already encrypted by the access point's WPA2. The HTTPS listener
for the site network will set the attribute on its own cookies.

**A doubling delay per client, never a lockout.** Three failures cost
nothing, because mistyping a twenty-character secret is ordinary; after
that each attempt waits 1 s, 2 s, 4 s and so on to a five-minute ceiling,
and success clears the history. A hard lockout was rejected because it
hands anyone on the site network a denial of service: fail deliberately a
few times and the installer is locked out too. Here the *cost* of guessing
explodes while a legitimate operator is only ever delayed. Attempts refused
during a block are not counted, so hammering cannot extend someone else's
wait. Clients are keyed by source address — the appliance is never behind a
proxy, so forwarding headers would be attacker-controlled and are not
consulted.

Verification is additionally **serialized process-wide**. One Argon2id hash
claims 19 MiB by design; a handful of parallel attempts would claim that
much each and exhaust the appliance, turning the very cost that makes
guessing expensive into the way to take the unit down. The hash also runs
on a blocking thread rather than on the async runtime, so a login in
progress does not stall the liveness probe.

**Deny by default.** Exactly two routes are open — the liveness probe,
which reveals nothing, and the login endpoint itself. Protected routes are
grouped behind the session guard, so a route added to that group is
protected by construction. The inverse arrangement would mean a route is
exposed whenever someone forgets, which is the wrong direction for a
mistake to fall.

The secret travels in the request body, never in a query string: URLs reach
server logs, browser history and referrer headers. The same reasoning fixes
the contract for the QR code printed on the label — it encodes the secret
in the URL **fragment** (`https://…/#s=…`), which browsers never transmit,
so the dashboard reads it client-side, exchanges it for a session and
clears it from the address bar.

### Consequences

- Good: revocation is immediate, and no token exists at rest.
- Good: guessing the secret over the network is uneconomic — under the
  five-minute ceiling a client gets at most a few hundred attempts a day
  against a 100-bit secret — without any way for a third party to lock the
  installer out.
- Good: memory used by authentication is bounded from every direction:
  one hash at a time, a capped session store, a capped client history.
- Bad: sessions do not survive a reboot, so a power cut mid-installation
  means logging in again. The QR code makes that a two-second affair, and
  the alternative — writing bearer tokens to the card — is worse.
- Bad: keying the throttle on the source address is weak on a network where
  an attacker can pick addresses freely. It raises cost rather than
  preventing attack, which is all a throttle can do; the real defence is
  the 100 bits of the secret.
- Bad: serializing verification means many simultaneous logins queue. With
  a handful of legitimate users this is invisible, and the queue is
  precisely what keeps memory bounded.
