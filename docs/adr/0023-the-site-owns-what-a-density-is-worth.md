# The site owns what a density is worth

- Status: accepted
- Date: 2026-08-05
- Amends: [0020](0020-model-bundles-and-the-appliance-library.md)

## Context and Problem Statement

Turning a density class into a waiting time takes seven numbers: the analysis
window and hop the model was trained under, how many people each class
represents, the service rate λ, and three thresholds that shape the display.
All seven travelled together in the bundle a training run produced, and
activating a model wrote all seven into the appliance configuration.

That arrangement was wrong in two ways, and only the first was visible.

**It stored the same values twice.** Once in the bundle, once in the
configuration, with nothing saying which was authoritative. Re-importing a
model silently replaced whatever the site had settled on.

**More seriously, it shipped a guess as a result.** Nothing in a training run
counts heads. A labelled capture says "the queue was medium at 12:04"; it never
says how many people that was. `people_per_class` therefore could not come from
training, and λ — how fast a till serves — is not a property of a model at all:
the same model stays valid when a second till opens, while the waiting time
halves.

## Considered Options

- Keep one tuning, and let the bundle seed it on first import only.
- Keep one tuning, and let the site override individual fields.
- Split by owner: what the model dictates, and what the site answers.

## Decision Outcome

**Each number has one owner, and nothing is stored twice.**

| Value | Owner | Where it lives |
|---|---|---|
| `window_us`, `hop_us` | the training run | `analysis.json` in the bundle, copied beside the model in service |
| people per class, λ | the operator | `wait` in the appliance configuration |
| smoothing, hysteresis, reliability threshold | the operator | same, with defaults nobody has to touch |

Activating a model no longer writes anything about the site. The intake reads
the analysis geometry from beside the model it loads, so it never has to know a
library exists — the same reason the model itself is copied rather than pointed
at.

The two halves are answered in different places because they are known at
different times: an appliance is installed weeks before its first model comes
back from training, and the queue can be described on the day.

### What the operator is asked

Two observable questions per density class — what the queue looks like, and how
many people that is — and one for the site: how many people a till serves per
minute. Nothing is defaulted. A head count nobody entered would produce waiting
times that look measured.

This adds a step to the guided installation, between the network and the
calibration recording: the words are needed before someone spends twenty
minutes pressing them.

## Consequences

A model trained at one site and imported at another no longer carries the first
site's service rate with it.

Re-importing a model leaves the operator's answers alone, so improving a model
is not a reason to re-describe a queue.

`site.json` becomes `analysis.json` and holds two fields. Bundles produced
before this change are refused rather than misread: the appliance asks for a
member it will not find, which is a clearer failure than a window silently
defaulted.

Little's Law stays the mechanism — `W = L / λ` — with L read from the density
and λ answered by the site. What changed is who supplies each term.
