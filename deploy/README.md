# Déploiement Mariam Flow

Ce dossier contient les assets de déploiement.

## Installation (sur Raspberry Pi)

Depuis le dossier de release extrait :

```bash
chmod +x deploy/install.sh
./deploy/install.sh
```

Variables utiles :
- `MARIAM_FLOW_PREFIX` (défaut: `/opt/mariam-flow`)
- `MARIAM_FLOW_USER` (défaut: `pi`)
- `MARIAM_FLOW_GROUP` (défaut: `pi`)

Pré-requis :
- `python3` + `python3-venv`
- `curl`

## Services systemd

- `mariam-model.service` : service Python local (modèle)
- `mariam-flow.service` : service Rust (capteurs + API)

Commandes utiles :

```bash
sudo systemctl status mariam-flow
sudo systemctl status mariam-model
sudo systemctl restart mariam-flow
sudo systemctl restart mariam-model
journalctl -u mariam-flow -f
journalctl -u mariam-model -f
```
