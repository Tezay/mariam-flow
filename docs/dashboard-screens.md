# Dashboard screens

How the dashboard is built and laid out is in [dashboard.md](dashboard.md).

## Following the appliance's phase

The shell follows the appliance's phase rather than guessing: during
installation it is a full-frame wizard, and the tabbed shell appears only once
the installation is closed. Translations are dictionaries in the repository —
English is the reference, other locales are typed against it, so a missing key
fails the build rather than the customer.

The wizard shows the first step the appliance's readiness leaves unsatisfied,
never a position the browser advances: an installation interrupted mid-step
resumes where the stored facts stand, whatever the browser was showing. Its
stepper doubles as the only navigation — a finished step is a button back to
itself, which also lets it show steps satisfied out of order. Each screen is
one question, with a reserved frame beside it on a wide screen and above it on
a phone; the frame holds its space whether or not it has artwork in it.

Its fourth step asks what the queue looks like at each density, how many
people that is, and how fast it is served — the answers that turn a density
into a waiting time (ADR 0023). It comes before the recording because the
words are needed before someone spends twenty minutes pressing them, and
because nothing in a training run can supply them: a labelled capture says the
queue was medium, never how many people that was. Nothing is defaulted, since a
head count nobody entered would produce waiting times that look measured.

The same component serves the settings, so a site that opens a second till
corrects one number rather than reinstalling. The three thresholds that shape
the display — smoothing, hysteresis, the reliability floor — are folded away
there and never asked during an installation.

Its last step records the site's first capture, because nothing else can:
closing an installation requires one, and the screen that records later ones
only appears once the installation is closed.

The tabbed shell is sized to the viewport and scrolls its content, not the
document: the tabs are part of the frame rather than something the reader has
to scroll back to. That is what pins them, rather than a fixed position paired
with a padding that would have to agree with their height from somewhere else.
Because a tab can now be reached while the content is scrolled, changing tab
returns the surface to its own top. Printing neutralises the arrangement — a
scroll container has no equivalent on paper, and the network request would
otherwise print clipped to what happened to be on screen.

## Sensors

The sensors screen is where a failing installation is diagnosed and repaired.
It reports the transmitter first — a fault there explains every row beneath it —
then each receiver with its rate, where it sits, and how long it has been quiet.
Silence is measured against the newest frame of the whole stream rather than
against the clock, because every sensor stopping is a different fault from one
sensor stopping, and only the comparison between them tells the two apart.

Sensors appearing and going quiet are journalled, so the question the screen
answers in the present — which sensor is silent — has an answer in the past
too. Silence is measured against the newest frame of the stream *or the clock,
whichever is later*: against the stream alone a node cannot lag itself, so an
installation with one receiver could never report it silent, and one where
every receiver stopped would report them all healthy.

A failed node is replaced one at a time, keeping its identifier (ADR 0021): the
identifier is what capture sessions are written against and what a density model
was validated for, so a receiver renumbered by a repair would leave the site
holding a model that no longer fits it. Where a sensor sits is likewise a
property of the installation rather than of one capture, described once here and
copied into every recording afterwards.

## Journal

The journal is readable from the settings, paged **on the row rather than on
an offset**: it is written while it is read, and an offset would make events
arriving mid-read repeat some rows and skip others. A poll asks what has
arrived since the newest row on screen and reports the count; nothing is merged
until the reader asks for it, because rows appearing under the eye of someone
reading an incident is what makes a journal hard to read. Rows are grouped by
day in the reader's own zone, and an unknown family in the query reads as no
filter — it can only come from a hand-written URL, and an empty journal would
look like an appliance that had never done anything.

## Settings

The settings screen is a list of sections resolving to one detail — Site,
Network, Queue, Service hours, System, Journal. On a phone the list is the
screen until a section is chosen; on a wide screen it is a rail beside the
detail. One component, two shapes, so a section added later lands somewhere
rather than lengthening a single page.

## Calibration

Recording a capture is one component, used by the wizard's fourth step and by
the calibration screen alike. It owns the live subscription, the rule that says
a receiver has gone quiet, the description and placements a capture is written
against, and the full-frame labelling surface — so an installation and a later
campaign cannot drift into recording different things. Only the words above the
form differ: the wizard adds a line saying what the operator is about to do,
because that is where labelling is met for the first time.

Below it, the calibration screen holds the two ends of the training loop side
by side on a wide screen and stacked on a phone: the models the appliance
holds, and the recordings the next one will be trained from. Each carries its
own history, so neither reads as a step of the other. The model in service is
the head of the list it belongs to rather than a card above it — there is one
collection, and one of its members is in use.

Both histories page at the same length, through the same control, and the model
library is held by the shell rather than fetched by each screen that shows it:
renaming a model on one tab must not leave another naming it the old way.

## Shared pieces

The shared pieces — the button, the surface, the section header, the pager, the
save confirmation, the dialogs — are defined once under `components/ui` and
used everywhere else. A screen that needs a control it does not have gains a
variant there rather than a set of classes of its own, which is what keeps two
screens built months apart from drifting apart.
