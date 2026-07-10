# One trained model per site

- Status: accepted
- Date: 2026-07-10

## Context and Problem Statement

CSI measures how a specific room's multipath propagation is perturbed: the
signal encodes wall geometry, furniture, and node placement. A model trained
at one site therefore learns, in large part, that site's radio environment.
The literature documents severe accuracy collapse when CSI models are
applied across sites without adaptation (domain shift). A deployment
strategy for models across sites is needed.

## Considered Options

1. One model per site: shared pipeline, per-site trained weights
2. A single global model deployed everywhere
3. A global pretrained model, fine-tuned per site

## Decision Outcome

Chosen option 1. Everything reproducible is shared across sites — feature
extraction, training pipeline, hyperparameters, evaluation protocol. What is
learned from data is per-site: model weights, the class-to-people-count
mapping, and the service rate λ per time slot. Each site's supervised
calibration session produces that site's training set, and its model is
trained from scratch.

Option 2 is incompatible with the physics: cross-site distribution shift is
the dominant error source, not a tuning detail. Option 3 is the desirable
long-term direction (pretraining across sites, few-shot adaptation to a new
one) but is not actionable before data from several sites exists, and
fine-tuning is largely meaningless for the v1 model class (regularized
linear / gradient-boosted classifiers). The per-session dataset format
(ADR 0001) is what will make option 3 possible later.

### Consequences

- Good: each site's model fits its actual radio environment; no silent
  cross-site degradation; simple, auditable training runs.
- Bad: every new site requires a supervised calibration pass before going
  live; model artifacts must be managed per site (versioning, storage,
  deployment).
