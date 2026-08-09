# The training run reports its own evaluation

- Status: accepted
- Date: 2026-08-09
- Amends: [0020](0020-model-bundles-and-the-appliance-library.md)

## Context and Problem Statement

A training run measures how well its model performs — accuracy against a
majority-class baseline, and a confusion matrix over the four density classes,
all obtained by holding whole sessions out. It printed that to the terminal and
kept nothing.

The bundle it shipped carried the weights, the analysis geometry, and a
manifest naming the run. So an appliance holding three models could say when
each arrived and how wide a window each expects, and nothing at all about
whether any of them works.

That gap falls exactly where a decision has to be made. Recalibrating a site
means importing a new model beside one already in service, and choosing. The
person making that choice stands in front of the appliance; the numbers that
would inform them were on a laptop, weeks earlier, in a scrollback buffer.

The appliance cannot recompute them. It holds neither the captures a model was
trained on — they are exported and often deleted — nor a training runtime; the
board it runs on has 512 MB and no Python.

A related gap: the manifest recorded *how many* sessions a run used, never
which ones. "This model came from the June campaign" was not a question the
data could answer.

## Considered Options

- **Leave it outside.** Read the scores where they were printed, keep a note.
  Costs nothing, and puts the decision back where the operator is not.
- **Ship rendered figures.** Put the matplotlib evaluation figure in the
  bundle as a PNG. Immediate, but an image cannot be read by a screen reader,
  does not respond to a phone's width, and cannot be compared against another
  model's by anything but eye.
- **Ship the numbers.** A structured `evaluation.json` member, rendered by
  whoever reads it.

## Decision Outcome

**The evaluation ships with the weights, as data.**

A bundle gains a fourth member, `evaluation.json`, written by the same run that
computed it — there is no second computation that could disagree with the
first:

```json
{
  "schema": 1,
  "accuracy": 0.674,
  "baseline_accuracy": 0.312,
  "confusion": [[612, 74, 11, 3], …],
  "windows": 2841,
  "splits": 4,
  "receivers": ["rx-1", "rx-2"],
  "sessions": [{ "session_id": "kit-20260805T143000Z", "windows": 712,
                 "support": [180, 190, 171, 171] }]
}
```

**The confusion matrix's orientation is part of the format**: rows are truth,
columns are prediction, in `empty, low, medium, saturated` order. Read the
other way round every conclusion drawn from it is inverted, so it is stated
once here rather than restated by each reader.

**`sessions` names the captures**, which is what ties a model back to the
recordings still held on the appliance. The manifest's count is derived from
the same list, so the two cannot disagree; a capture that yielded no usable
window appears in neither, having taken no part.

Nothing about the site enters this member. It says how a model performed, never
what a density is worth here — that remains the operator's, by ADR 0023.

### Compatibility, in both directions

The member is **additive and optional**. Staging already skips archive members
it does not recognise, and the manifest is already read as absent-or-present,
so:

- a bundle produced before this change imports unchanged, and reports no
  evaluation rather than being refused — refusing it would strand a model that
  estimates perfectly well;
- a bundle produced after it imports on an appliance that predates it, which
  ignores the member.

The payload declares its `schema`. An appliance meeting a version it does not
know reports the model as carrying no evaluation, rather than reading fields
until something fits and presenting the result as scores.

## Consequences

The appliance answers `GET /api/models/{id}` with one model and its evaluation.
The listing carries `has_evaluation` alone: a library is read on every visit to
the calibration screen, and a matrix is read once someone has chosen a model.

What a reader is shown is derived from the matrix rather than restated from it —
whether an empty zone is told from an occupied one, whether mistakes land on a
neighbouring class, whether the exact level is found. A single accuracy figure
says whether a model is right, never how it is wrong, and the two lead to
different decisions: a model that confuses adjacent levels but never misses a
presence is worth deploying with coarser classes, while one scoring the same by
answering the commonest class is worth nothing.

The training run remains off the appliance. This changes what comes back from
it, not where it happens.
