# Cinema OV Watcher

Findet neue OV/OmU-Vorstellungen (englische Originalfassungen) im
Cineplexx Linz und Hollywood Megaplex PlusCity, postet Telegram-Alerts
im öffentlichen Kanal [@ov_linz](https://t.me/ov_linz) und zeigt alle
kommenden OV-Vorstellungen auf einer Webseite.

Rust-Backend (axum + Postgres) mit React-Frontend. Laufzeit, Genre und
Filmplakat werden direkt von den Kinoseiten gelesen und auf der Webseite,
in den Telegram-Alerts und im Kalender-Feed (`/showings.ics`) angezeigt.

## Lokal laufen lassen

```bash
docker compose up -d db
export DATABASE_URL=postgres://ov:ov@localhost:5432/ov
cd backend && cargo run            # http://localhost:8080
cd frontend && npm install && npm run dev   # Dev-Server mit Proxy (optional)
```

Telegram-Bot: bei @BotFather anlegen, Token notieren. Der Bot postet im
öffentlichen Kanal @ov_linz: dazu den Bot im Kanal als Administrator mit
Recht „Nachrichten posten" hinzufügen und als `TELEGRAM_CHAT_ID` einfach
`@ov_linz` eintragen.

## Tests

```bash
cd backend && cargo test           # braucht DATABASE_URL (docker compose up -d db)
cd frontend && npm test
```

## Docker

```bash
docker compose up --build          # App + Postgres, http://localhost:8080
```

## Kubernetes

```bash
kubectl apply -f k8s/pvc.yaml -f k8s/postgres.yaml -f k8s/secret.yaml \
  -f k8s/configmap.yaml -f k8s/deployment.yaml -f k8s/service.yaml
```

`k8s/secret.yaml` aus `k8s/secret.example.yaml` erzeugen und die echten
Werte eintragen (nicht committen).
