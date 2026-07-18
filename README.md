# Cinema OV Watcher

Findet neue OV/OmU-Vorstellungen (englische Originalfassungen) im
Cineplexx Linz und Hollywood Megaplex PlusCity, schickt Telegram-Alerts
und zeigt alle kommenden OV-Vorstellungen auf einer Webseite.

## Lokal laufen lassen

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements-dev.txt
DATA_DIR=./data \
TELEGRAM_BOT_TOKEN=... TELEGRAM_CHAT_ID=... \
.venv/bin/python -m app.main
# Webseite: http://localhost:8080
```

Telegram-Bot: bei @BotFather anlegen, Token notieren; Chat-ID z. B. über
@userinfobot.

## Tests

```bash
.venv/bin/pytest -q
```

## Docker

```bash
docker build -t cinema-ov-watcher:latest .
docker run --rm -p 8080:8080 \
  -e TELEGRAM_BOT_TOKEN=... -e TELEGRAM_CHAT_ID=... \
  -v ov-data:/data cinema-ov-watcher:latest
```

## Kubernetes

```bash
kubectl apply -f k8s/pvc.yaml -f k8s/secret.yaml -f k8s/configmap.yaml \
  -f k8s/deployment.yaml -f k8s/service.yaml
```

`k8s/secret.yaml` aus `k8s/secret.example.yaml` erzeugen und die echten
Werte eintragen (nicht committen).
