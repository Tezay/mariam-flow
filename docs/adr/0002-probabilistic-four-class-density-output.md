# Density model output: probability distribution over four ordered classes

- Status: accepted
- Date: 2026-07-10

## Context and Problem Statement

The system must estimate the crowd density of the monitored zone from CSI.
The underlying physical quantity (number of people) is continuous, so the
output space of the model is a genuine design decision.

Two constraints dominate. First, supervision: ground truth is either a human
selecting one of a small number of levels, or an exact people count from a
reference sensor during calibration. Second, signal resolution: the CSI
response saturates as density grows — each additional person perturbs
multipath propagation less than the previous one, so the effective
information content of indoor CSI supports a coarse density scale, not an
exact count. Published exact-counting results degrade beyond small groups
even in controlled environments.

## Considered Options

1. Classifier over four ordered classes, exposing the full probability
   distribution
2. Regression of the exact people count
3. Classifier over four classes, exposing only the predicted class (argmax)
4. Direct regression of the waiting time

## Decision Outcome

Chosen option 1: the model is a 4-class classifier
(`empty < low < medium < saturated`, encoding frozen in `flow-core`), and
its canonical output is the full probability vector. The discrete class is
the argmax, used for labels and display only. Downstream consumers operate
on the distribution:

- expected people count `E[L] = Σ pᵢ · L(classᵢ)` feeds Little's Law, so the
  wait estimate evolves continuously instead of jumping at class boundaries;
- the distribution provides a confidence measure, used to gate the published
  estimate and to detect model drift;
- smoothing and hysteresis operate on a continuous signal.

Option 2 (count regression) puts the largest errors exactly where the signal
saturates (high density) while claiming a precision the signal does not
contain; it remains a possible future experiment, enabled at zero capture
cost because labels store the exact count when a reference sensor provided
it (see ADR 0001). Option 3 discards calibration, confidence, and the
continuous signal for no gain. Option 4 conflates the state of the queue
(observable in CSI) with the service rate (not observable in CSI), making
the learned mapping non-stationary under staffing changes; Little's Law
separates these concerns explicitly (see `docs/architecture.md`).

### Consequences

- Good: output matched to the effective resolution of the signal; continuous
  downstream signal and confidence for free; count-labeled sessions keep
  finer class granularities or regression experiments reachable without
  recapture.
- Bad: class boundaries are site-specific parameters that must be chosen and
  documented per site; plain multinomial training ignores class order
  (an ordinal loss is a possible later refinement).
