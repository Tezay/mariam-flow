# Dashboard screens

How the dashboard is built and laid out is in [dashboard.md](dashboard.md).

## Following the appliance's phase

The shell shows what the appliance's phase calls for: a full-frame wizard
during installation, and the tabbed shell once the installation is closed.
Translations are dictionaries in the repository. English is the reference and
other locales are typed against it, so a missing key fails the build.

The wizard shows the first step the appliance's readiness leaves unsatisfied.
The browser keeps no position of its own, so an installation interrupted
mid-step resumes where the stored facts stand. The stepper is the only
navigation: a finished step is a button back to itself, and steps satisfied
out of order show as such. Each screen asks one question, with a reserved
frame beside it on a wide screen and above it on a phone.

The fourth step asks what the queue looks like at each density, how many
people that is, and how fast it is served. These answers turn a density into a
waiting time (ADR 0023). The step comes before the recording because the words
are needed before someone spends twenty minutes pressing them, and because no
training run can supply them: a labelled capture says the queue was medium,
not how many people that was. Nothing is defaulted, since a head count nobody
entered would produce waiting times that look measured.

The same component serves the settings, so a site that opens a second till
corrects one number without reinstalling. The three thresholds that shape the
display (smoothing, hysteresis, the reliability floor) are folded away there
and are not asked during an installation.

The last step records the site's first capture. Closing an installation
requires one, and the screen that records later ones only appears once the
installation is closed.

The tabbed shell fills the viewport and scrolls its content, not the document,
which keeps the tabs in place without a fixed position. Changing tab returns
the surface to its top. Printing undoes the arrangement, since a scroll
container has no equivalent on paper and the network request would print
clipped.

## When the appliance stops answering

Every screen reads one shared live stream, and its silence is what the page
watches for: a connection whose other end has vanished stays open and reports
nothing. After five quiet seconds a notice says since when, above every
screen. What was last shown stays readable, dimmed and out of reach.

The page then asks the appliance for its status every few seconds and, on an
answer, replaces the connection, since a browser retries a dropped stream but
not a refused one. An appliance that answers without knowing the session has
restarted, so the page returns to the sign-in. A tab back from the background
is given the same five seconds first.

The same place says when the appliance clock is more than two minutes from the
device's. The appliance has no battery-backed clock, and dates what it records.

## Live

The waiting time is shown only while it is current. The appliance keeps its
last estimate until another replaces it. Past ten seconds, the threshold that
also calls a receiver silent, the screen says the estimation is interrupted,
names the receivers not answering, and keeps the last figure beside its time.
What the appliance is doing is worked out in one place from the live stream,
for this screen and the system section alike.

Beneath it the history is drawn on a time axis ending at the present. A minute
nothing was estimated in shows as a break in the curve and a hatched stretch
of the band. The history is read again as each minute ends.

## Sensors

The sensors screen is where a failing installation is diagnosed and repaired.
It reports the transmitter first, since a fault there explains every row
beneath it, then each receiver with its rate, where it sits, and how long it
has been quiet.

Silence is measured against the newest frame of the stream or the clock,
whichever is later. Against the stream alone a node cannot lag itself: an
installation with one receiver could never report it silent, and one where
every receiver stopped would report them all healthy. Comparing receivers with
each other is still what tells one dead sensor from a dead network. Sensors
appearing and going quiet are journalled, so the same question can be asked of
the past.

A failed node is replaced one at a time and keeps its identifier (ADR 0021).
Capture sessions are written against that identifier and a density model is
validated for it, so a receiver renumbered by a repair would leave the site
with a model that no longer fits. Where a sensor sits also belongs to the
installation: it is described here or as a recording starts, and copied into
every recording afterwards.

## Journal

The journal is read from the settings and paged on the row, not on an offset:
it is written while it is read, and an offset would repeat some rows and skip
others as events arrive. A poll asks what has arrived since the newest row on
screen and reports the count. Nothing is merged until the reader asks, because
rows appearing under the eye of someone reading an incident make a journal
hard to read. Rows are grouped by day in the reader's own zone. An unknown
family in the query reads as no filter: it can only come from a hand-written
URL, and an empty journal would look like an appliance that had never done
anything.

## Settings

The settings screen is a list of sections resolving to one detail: Site,
Network, Queue, Service hours, System, Journal. On a phone the list is the
screen until a section is chosen. On a wide screen it is a rail beside the
detail. A section added later lands in the list and does not lengthen a single
page.

## Calibration

The calibration screen leads with the model in service, the only filled
surface on it, or says there is none. Beneath come *Prepare the next model*,
holding the loop in the order it is walked (start a recording, the recordings
so far, the import that ends it), and *Kept models*, the earlier ones, any of
which can be put back into service. They sit side by side on a wide screen and
stack in that order on a phone.

Recording and importing each open a dialog that says what is about to happen
before the button that does it. The screen carries no form.

Recording a capture is one component, used by the wizard's last step and by
this screen alike, so an installation and a later campaign cannot drift into
recording different things. Before anything is recorded it shows each paired
sensor with where it sits and whether it is answering. That includes a receiver
that never streamed, which the stream does not mention.

Both histories page at the same length, through the same control. The model
library is held by the shell, not fetched by each screen that shows it, so
that renaming a model on one tab cannot leave another naming it the old way.

Opening a recording or a model replaces the surface and adds no tab. Each is
consulted one at a time, and an operator who only configures the appliance
never sees these screens.

### A recording

The page reads in the order the questions arise: how long the capture ran and
how many receivers streamed, then anything wrong with it, then the capture
itself, then how the marked levels compare. Concerns are listed, not scored: a
silent receiver and a hole in the middle are both fatal, and send the reader
to different places.

The label track, the heatmaps and the feature chart are panels of one
component that owns a single visible time range and cursor position. Dragging
across the chart zooms every panel, one button returns to the whole capture,
and a crosshair marks the same instant throughout. The panels are aligned by
measurement: uPlot sizes its axis gutter from the tick labels it draws, so
that width is read back after layout and the other panels are inset by it.

Zooming re-slices the stored columns and fetches nothing finer, so the panel
states how much time one column covers. Heatmaps use the same viridis ramp and
colour bar as the Python session report. The ramp is perceptually uniform,
where a single hue runs out of distinguishable steps around six.

The chart tints its background by marked level, which shows at a glance
whether the measurement moves when the level changes. It shows one measurement
at a time across all receivers, because the seven are on different scales.
Beneath, it summarises that measurement per marked level and names the levels
whose spreads overlap, since no threshold separates two means whose spreads
overlap.

### A model

The detail goes from the general to the technical: four figures, then three
sentences on what the model can and cannot do, then the confusion matrix and
the captures it was trained on, both behind a disclosure. The matrix is a table
with its truth in the rows and its headers marked as such, so it can be read
without sight. Intensity carries the share within a row and weight carries the
diagonal, so it can be read without colour.

Comparing a model with another is a screen of its own. It is offered from a
model that is not in service, and again when an import puts a new one in, the
two moments the question is asked. Each measure is named once with its two
values against it, and each difference is stated once, on the candidate. Where
three columns fit, that is a table. Where they do not, it is a stack naming
both models against every figure, since a table on a phone would cut the
figures.

Levels are reported one by one as well as in aggregate, each with the windows
it rests on. A model that improved overall may have lost the rarest level,
which is the costliest to miss and rests on the thinnest evidence. The
recordings both runs were scored on are listed side by side, with a dash
marking one a model never saw.

## Shared pieces

The button, the surface, the section header, the pager, the save confirmation
and the dialogs are defined once under `components/ui` and used everywhere
else. A screen that needs a control it does not have gains a variant there, not
a set of classes of its own, so that two screens built months apart do not
drift apart.
