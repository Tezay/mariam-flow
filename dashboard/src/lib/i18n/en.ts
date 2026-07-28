/**
 * The reference dictionary.
 *
 * Every other locale is typed against this one, so a missing translation is
 * a compile error rather than a string that silently falls back to English
 * in front of a customer. The parity test covers the other direction —
 * keys that exist in a translation but no longer here.
 */
export const en = {
  'app.name': 'Mariam Flow',
  'app.loading': 'Connecting to the appliance…',
  'app.retry': 'Try again',
  'app.unreachable': 'The appliance is not responding.',

  'locale.switch': 'Passer en français',

  'login.title': 'Sign in',
  'login.lead': 'Enter the secret printed on the appliance label, or scan its QR code.',
  'login.secret': 'Device secret',
  'login.secretPlaceholder': 'XXXX-XXXX-XXXX-XXXX-XXXX',
  'login.submit': 'Sign in',
  'login.working': 'Checking…',
  'login.invalid': 'That secret is not right.',
  'login.throttled': 'Too many attempts. Try again in {seconds} s.',
  'login.failed': 'Sign-in failed. Check the connection to the appliance.',
  'login.scanned': 'Secret read from the QR code.',

  'shell.signOut': 'Sign out',

  'tab.live': 'Live',
  'tab.nodes': 'Nodes',
  'tab.calibration': 'Calibration',
  'tab.settings': 'Settings',

  'status.kit': 'Kit',
  'status.site': 'Site',
  'status.siteUnnamed': 'Not named yet',
  'status.model': 'Model',
  'status.modelInstalled': 'Installed',
  'status.modelMissing': 'None',
  'status.activity': 'Activity',
  'status.network': 'Network',
  'status.sensorAp': 'Sensor network',
  'status.channel': 'channel {channel}',
  'status.uplink': 'Site uplink',

  'uplink.undecided': 'Not configured',
  'uplink.offline': 'Offline',
  'uplink.wifi': 'Wi-Fi',
  'uplink.ethernet': 'Wired',

  'runtime.idle': 'Idle',
  'runtime.calibrating': 'Calibrating',
  'runtime.live': 'Estimating',

  'nodes.title': 'Sensing nodes',
  'nodes.none': 'No node paired yet.',
  'nodes.role.tx': 'Transmitter',
  'nodes.role.rx': 'Receiver',

  'header.language': 'Switch to French',
  'header.clock': 'Use a 12-hour clock',
  'header.clock24': 'Use a 24-hour clock',

  'live.wait': 'Waiting time',
  'live.minutesShort': 'min',
  'live.confidence': '{value}% confidence',
  'live.unreliable':
    'Below the site confidence threshold — this estimate is not published to anyone.',
  'live.warmingUp': 'Filling the first analysis window…',
  'live.notEstimating': 'Not estimating. Check the model and the site calibration.',
  'live.history': 'Last hour',
  'live.historyEmpty': 'No history yet.',
  'live.showTable': 'Table',
  'live.showChart': 'Chart',
  'live.time': 'Time',
  'live.class': 'Level',
  'live.peak': 'Peak {value} min',
  'live.stream': 'Sensor stream',
  'live.noNodes': 'No sensor is streaming.',
  'live.framesPerSecond': '{value} frames/s',
  'live.silent': 'silent for {seconds} s',
  'class.empty': 'Empty',
  'class.low': 'Low',
  'class.medium': 'Medium',
  'class.saturated': 'Saturated',

  'wizard.progress': 'Step {current} of {total}',
  'wizard.stage.site': 'Name the site',
  'wizard.stage.nodes': 'Connect the sensors',
  'wizard.stage.network': 'Connect to the network',
  'wizard.stage.calibration': 'Calibrate',
  'wizard.stage.complete': 'Ready',
  'wizard.stage.site.lead': 'Give this installation a name you will recognise later.',
  'wizard.stage.nodes.lead': 'Power the three sensors and wait for them to appear.',
  'wizard.stage.network.lead':
    'Choose how the appliance reaches the site network, or stay offline.',
  'wizard.stage.calibration.lead': 'Teach the appliance what a busy queue looks like at this site.',
  'wizard.stage.complete.lead': 'Everything is set. The appliance is ready to serve.',
  'wizard.comingNext': 'This step is not built yet.',
} as const;
