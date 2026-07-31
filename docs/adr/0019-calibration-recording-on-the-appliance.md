# Calibration recording on the appliance

- Status: accepted
- Date: 2026-07-31

## Context and Problem Statement

A site cannot be estimated before it has been calibrated: CSI is site-specific,
so every installation needs its own recorded, labeled capture to train from
(ADR 0003). Until now that capture was made with a command-line tool, which is
not something an installer standing in a canteen can run.

Two questions follow. How does a capture take the frame stream, which the
inference pipeline is also reading? And what does a recorded capture mean for
an installation that is not yet able to estimate anything?

## Considered Options

For the stream: a second reader alongside the pipeline, a separate process, or
another stage of the intake loop.

For the installation: hold the wizard open until a model exists, let the
installer close it with the last step outstanding, or make the recording itself
the step.

## Decision Outcome

**Recording is another stage of the intake loop.** Reading the stream and
estimating from it were already separated (ADR 0017), which leaves exactly one
place where frames arrive and stages attach to them. A capture attaches there
too, and the runtime's existing rule — one consumer at a time — keeps it
exclusive of estimation.

The consequence is that **estimation resumes on its own**. The estimator is
never torn down, only skipped while a capture holds the stream, the same way it
is skipped outside service hours. Nothing has to remember whether estimation was
running beforehand, because nothing stopped it.

Two details of the recording are deliberate:

- **The session directory is created by the handler, not the intake thread.**
  A full card or a name already taken is then answered to the caller instead of
  failing out of sight. Writing happens on the thread that has the frames.
- **Labels are stamped by the appliance clock.** The phone doing the labelling
  and the appliance recording the frames are two machines; a label has to land
  on the same timeline as the frames it describes.

## Recording finishes the installation, the model does not

**A recorded session satisfies the last step of the wizard.** The model does
not, and this is the decision with the most consequence.

Recording produces data, not a model: the session is exported, trained off site,
and imported back as ONNX days later. Gating the installation on a model would
therefore keep the appliance in its full-frame wizard for as long as training
takes — and the wizard has no navigation, so the site would have no access to
the live view, the service hours, the sensor health or the network settings in
the meantime. An installation that cannot be closed on the day it is performed
is a wizard that is wrong about what installing means.

Installation readiness therefore carries two separate facts. `site_captured` is
what the wizard waits on. `model_ready` is reported, drives what the live view
says, and is satisfied later from the settings. Fitting the appliance and
bringing it into service are two visits, and the software now says so.

## Consequences

`site_captured` is a filesystem fact — is there a sealed session — and is set
the moment a capture is sealed rather than at the next boot: sealing is what
makes a session usable, and the installation waits on exactly that. A directory
still carrying the recording suffix does not count, since training on a
truncated file is worse than having nothing.

Session metadata is derived from the configuration wherever it can be: the
site, the paired nodes, the radio channel, the software version. Only what the
appliance cannot know is asked of the operator — where each node physically
sits, and what the density classes mean at this site. A node whose position is
left undescribed still records: an empty position is recoverable, a lost
capture is not.

The session identifier is sanitised as it is built rather than rejected
afterwards, because it *is* the directory name.
