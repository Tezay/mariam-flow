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

Opening either a recording or a model replaces the surface rather than adding a
tab: each is asked about one of them at a time, and an operator who only
configures the appliance is never shown that the screens exist.

### A recording

It reads in the order the questions arise: how long it ran and how many
receivers streamed, then anything wrong with it stated plainly, then the
capture itself, then how the marked levels compare. Concerns are listed rather
than scored — a silent receiver and a hole in the middle are both fatal, and
send the reader to different places.

The label track, the heatmaps and the feature chart are panels of one component
owning **a single visible time range and cursor position**: dragging across the
chart zooms every panel, one button returns to the whole capture, and a
crosshair marks the same instant throughout. Their alignment is measured rather
than assumed — uPlot sizes its axis gutter from the tick labels it ends up
drawing, so that width is read back after layout and the other panels are inset
by it.

Zooming re-slices the stored columns rather than fetching finer ones, so the
panel states how much time one column covers. Heatmaps carry the same
**viridis** ramp and colour bar as the Python session report — perceptually
uniform, where a single hue runs out of distinguishable steps around six.

The chart tints its background by marked level, so the question the screen
exists to answer — does the measurement move when the level changes — is read
in one place. It shows one measurement at a time across all receivers, the
seven living on scales nothing can share, and summarises it per marked level
beneath: levels whose spreads run into each other are named as such, two means
far apart with overlapping spreads being separated by no threshold.

### A model

The detail reads from the general to the technical — four figures, then three
sentences saying what the model can and cannot do, then the confusion matrix
and the captures it was trained on, both behind a disclosure. The matrix is a
table with its truth in the rows and its headers marked as such, so it can be
read without sight; intensity carries the share within a row and weight carries
the diagonal, so it can be read without colour.

## Shared pieces

The shared pieces — the button, the surface, the section header, the pager, the
save confirmation, the dialogs — are defined once under `components/ui` and
used everywhere else. A screen that needs a control it does not have gains a
variant there rather than a set of classes of its own, which is what keeps two
screens built months apart from drifting apart.
