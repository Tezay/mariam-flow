# Running the appliance locally

The edge components carry no board-specific dependency, so the whole appliance
runs on a development machine with no sensors attached. What follows brings up
one unit, feeds it simulated sensors, and opens its dashboard — the same
binary, the same code path, and the same dashboard the hardware serves.

Build prerequisites are in the [README](../README.md); the dashboard has to be
built once before the daemon can embed it:

```sh
cd dashboard && pnpm install && pnpm build && cd ..
```

## Provision a unit

Establishing an appliance means giving it a credential and a factory
configuration. One command does both:

```sh
cargo run -p flow-edge -- provision \
  --data-dir ./demo/data \
  --config ./demo/appliance.json \
  --kit-id KIT-0001 \
  --ap-ssid mariam-flow-0001 \
  --ap-passphrase choose-a-passphrase
```

It prints the device secret on stdout, alone, so it can be piped into a label
or a QR encoder. **Note it down**: only its hash is stored, and it cannot be
read back. Everything else the command says goes to stderr.

The access-point passphrase is supplied rather than generated because the
nodes are flashed with it before the appliance is provisioned; one invented
here would leave them unable to join. Neither the configuration nor the
credential is overwritten without `--force`, and the configuration is written
first, so a refused one never leaves a unit holding a secret that has already
been printed.

## Run it

```sh
cargo run -p flow-edge --features dashboard -- serve \
  --config ./demo/appliance.json \
  --data-dir ./demo/data \
  --reset-file ./demo/reset \
  --listen 127.0.0.1:8080
```

Then open <http://127.0.0.1:8080> and sign in with the secret. Opening
`http://127.0.0.1:8080/#s=THE-SECRET` signs in without typing it and clears
the fragment from the address bar — what scanning the label does.

Without the `dashboard` feature the daemon serves the API alone, which is
enough to exercise it with `curl`.

## Simulated sensors

Nothing joins the appliance on a development machine, so the installation
stops at pairing. An example streams CSI in the nodes' own format:

```sh
cargo run -p flow-ingest --example simulated-nodes -- \
  --from 127.0.0.1 --from 192.0.2.10
```

Each `--from` is one receiver. The appliance tells receivers apart by the
address a datagram comes from, so simulating two of them needs two addresses
this host actually holds — its loopback and its LAN address, for instance.
Within a few seconds the pairing step offers them, and the sensors screen
reports their rate.

The signal is a fixed pattern. It exercises intake, pairing, stream health and
capture; it says nothing about a queue, and no density or waiting time follows
from it.

## What is visible

Everything up to and including a recorded calibration: naming the site,
pairing, the network interview, capture with labelling, the session history
and its export, the model library, the settings, and the journal filling with
each of those.

Live estimation needs a trained model, which needs captures from a real site.
Until one is imported the supervision screen reports that it is not
estimating, which is the state a freshly installed appliance is really in.

## Cleanup

The daemon flushes its journal on `SIGTERM` and `SIGINT`, so stopping it with
Ctrl-C loses nothing. Everything it wrote is under the two paths given on the
command line; removing them returns the machine to where it started.
