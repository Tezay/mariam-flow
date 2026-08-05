# The appliance

An installed site runs one long-lived process, `flow-edge`, which embeds the
sensing and inference crates as libraries and adds what only a deployed unit
needs: its configuration, its installation lifecycle, and the dashboard an
installer works from (ADR 0008). The laboratory tools remain available for
exercising one stage of the chain in isolation.

## Board and network topology

The edge runs on any Cortex-A53 ARM64 Raspberry Pi board under Raspberry Pi OS
Lite 64-bit — Zero 2 W, 3 Model A+, 3 Model B+ — from one
`aarch64-unknown-linux-gnu` artifact. Nothing touches GPIO, I²C, a camera or a
GPU, so the board is chosen on availability. Interfaces are discovered rather
than named: a dongle is `wlan1` on one board where the built-in Ethernet is
`eth0` on another.

The appliance hosts the access point the sensing nodes join, on a fixed 2.4 GHz
channel matching the transmitter's: a station senses CSI only on the channel it
is associated with, so that channel cannot move. The built-in radio is
therefore dedicated to the sensor access point, and the site uplink runs on a
second interface — a USB Wi-Fi or USB Ethernet adapter (ADR 0009). Both uplink
kinds go through one code path.

The two networks are never bridged. Running with no uplink is a supported mode:
sensing, calibration and local display work unchanged, and only remote
supervision is unavailable. The configuration distinguishes an unanswered
uplink question from a deliberate choice to stay offline.

## Configuration and state

Configuration lives in one human-readable JSON file, validated on every load
and every save, and written atomically. It carries the appliance identity, the
sensor access point, the uplink, the paired nodes, the service hours, and what
this site turns a density into a waiting time with (ADR 0023).

Historical series — node health, estimates, events — live in SQLite instead,
where queries and retention are the natural operations.

Installation progress is derived from facts rather than stored as a cursor:
whether the site is named, the nodes are paired, the uplink question is
answered, the queue is described and a capture has been recorded. An appliance
interrupted mid-installation resumes where its configuration says it stands. A
single stored flag records that the installer closed the installation, so a
finished setup does not fall back into the wizard because a node is unplugged.

## Stream arbitration

Calibration and live inference both consume the single UDP stream from the
receivers, so at most one may hold it. Every start requires an idle stream, and
switching between the two requires stopping first. A refused request leaves the
running activity untouched.

## Reaching the site network

The installer is present but does not know what the network requires; the
network administrator knows but is not there. The appliance therefore
interviews the installer about what joining the network asks for — nothing, a
shared password, a personal account, a certificate, a sign-in page, or "I do
not know" — and derives the rest (ADR 0018).

Choosing the site Wi-Fi on a network the survey says needs an administrator is
refused with the reason, never stored as "offline". The wizard and the settings
ask the same questions from the same stored answers.

What the survey found is stored apart from the uplink: one says what the site
demands, the other what the appliance will do. A site requiring 802.1X leaves
the appliance offline because of what was found.

When the site has to act, the dashboard derives a request for its network
administrator — printable, and downloadable as Markdown. It is produced only
when action is required; an ordinary password-protected network needs nothing.
Rendering belongs to the dashboard rather than the appliance, which has neither
the memory for a document renderer nor a reason to hold two versions of the
same text.

Enterprise authentication is surveyed but not implemented. Credentials no code
can act on are not collected.

## The machine underneath

The appliance reports what the machine says about itself — board model,
operating system, kernel, uptime, load, memory and CPU temperature — read from
`/proc` and `/sys` rather than through a crate.

Every field is optional and an absent one is reported as absent: the same
binary is developed on a laptop that reports none of them.

Temperature is the figure worth watching, a board throttling long before it
stops.
