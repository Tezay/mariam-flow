import type { Messages } from './messages';

/**
 * French translation. Typed as [`Messages`], so the compiler refuses this
 * file the moment a key is added to the reference dictionary and not here.
 */
export const fr: Messages = {
  'app.name': 'Mariam Flow',
  'app.loading': 'Connexion au boîtier…',
  'app.retry': 'Réessayer',
  'app.unreachable': 'Le boîtier ne répond pas.',

  'locale.switch': 'Switch to English',

  'login.title': 'Connexion',
  'login.lead': "Saisissez le secret imprimé sur l'étiquette du boîtier, ou scannez son QR code.",
  'login.secret': 'Secret du boîtier',
  'login.secretPlaceholder': 'XXXX-XXXX-XXXX-XXXX-XXXX',
  'login.submit': 'Se connecter',
  'login.working': 'Vérification…',
  'login.invalid': "Ce secret n'est pas le bon.",
  'login.throttled': 'Trop de tentatives. Réessayez dans {seconds} s.',
  'login.failed': 'Échec de la connexion. Vérifiez la liaison avec le boîtier.',
  'login.scanned': 'Secret lu depuis le QR code.',

  'shell.signOut': 'Se déconnecter',

  'tab.live': 'Direct',
  'tab.nodes': 'Capteurs',
  'tab.calibration': 'Calibration',
  'tab.settings': 'Réglages',

  'status.kit': 'Kit',
  'status.site': 'Site',
  'status.siteUnnamed': 'Pas encore nommé',
  'status.model': 'Modèle',
  'status.modelInstalled': 'Installé',
  'status.modelMissing': 'Aucun',
  'status.activity': 'Activité',
  'status.network': 'Réseau',
  'status.sensorAp': 'Réseau des capteurs',
  'status.channel': 'canal {channel}',
  'status.uplink': 'Liaison au site',

  'uplink.undecided': 'Non configurée',
  'uplink.offline': 'Hors ligne',
  'uplink.wifi': 'Wi-Fi',
  'uplink.ethernet': 'Filaire',

  'runtime.idle': 'En attente',
  'runtime.calibrating': 'Calibration',
  'runtime.live': 'Estimation',

  'nodes.title': 'Capteurs',
  'nodes.none': 'Aucun capteur appairé pour le moment.',
  'nodes.role.tx': 'Émetteur',
  'nodes.role.rx': 'Récepteur',

  'header.language': 'Switch to English',
  'header.clock': 'Passer en 12 heures',
  'header.clock24': 'Passer en 24 heures',

  'live.wait': "Temps d'attente",
  'live.minutesShort': 'min',
  'live.confidence': '{value}% de confiance',
  'live.unreliable':
    "Sous le seuil de confiance du site — cette estimation n'est publiée à personne.",
  'live.warmingUp': "Remplissage de la première fenêtre d'analyse…",
  'live.notEstimating': 'Pas d’estimation. Vérifiez le modèle et la calibration du site.',
  'live.history': 'Dernière heure',
  'live.historyEmpty': 'Pas encore d’historique.',
  'live.showTable': 'Tableau',
  'live.showChart': 'Courbe',
  'live.time': 'Heure',
  'live.class': 'Niveau',
  'live.peak': 'Pic {value} min',
  'live.stream': 'Flux des capteurs',
  'live.noNodes': 'Aucun capteur n’émet.',
  'live.framesPerSecond': '{value} trames/s',
  'live.silent': 'silencieux depuis {seconds} s',
  'class.empty': 'Vide',
  'class.low': 'Faible',
  'class.medium': 'Moyenne',
  'class.saturated': 'Saturée',

  'wizard.progress': 'Étape {current} sur {total}',
  'wizard.stage.site': 'Nommer le site',
  'wizard.stage.nodes': 'Brancher les capteurs',
  'wizard.stage.network': 'Connecter au réseau',
  'wizard.stage.calibration': 'Calibrer',
  'wizard.stage.complete': 'Prêt',
  'wizard.stage.site.lead': 'Donnez à cette installation un nom que vous reconnaîtrez plus tard.',
  'wizard.stage.nodes.lead': 'Alimentez les trois capteurs et attendez qu’ils apparaissent.',
  'wizard.stage.network.lead':
    'Choisissez comment le boîtier rejoint le réseau du site, ou restez hors ligne.',
  'wizard.stage.calibration.lead':
    'Apprenez au boîtier à quoi ressemble une file chargée sur ce site.',
  'wizard.stage.complete.lead': 'Tout est en place. Le boîtier est prêt.',
  'wizard.comingNext': 'Cette étape n’est pas encore construite.',
};
