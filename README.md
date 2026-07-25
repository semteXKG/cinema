# Cinema OV Watcher

Findet neue OV/OmU-Vorstellungen (englische Originalfassungen) im
Cineplexx Linz und Hollywood Megaplex PlusCity, postet Telegram-Alerts
im öffentlichen Kanal [@ov_linz](https://t.me/ov_linz) und zeigt alle
kommenden OV-Vorstellungen auf einer Webseite.

## Lokal laufen lassen

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements-dev.txt
DATA_DIR=./data \
TELEGRAM_BOT_TOKEN=... TELEGRAM_CHAT_ID=... \
.venv/bin/python -m app.main
# Webseite: http://localhost:8080
```

Telegram-Bot: bei @BotFather anlegen, Token notieren. Der Bot postet im
öffentlichen Kanal @ov_linz: dazu den Bot im Kanal als Administrator mit
Recht „Nachrichten posten“ hinzufügen und als `TELEGRAM_CHAT_ID` einfach
`@ov_linz` eintragen (bei Kanälen ist die Chat-ID der @-Name).

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
