# Cinema OV Watcher

Findet neue OV/OmU-Vorstellungen (englische Originalfassungen) im
Cineplexx Linz und Hollywood Megaplex PlusCity, postet Telegram-Alerts
im öffentlichen Kanal [@ov_linz](https://t.me/ov_linz) und zeigt alle
kommenden OV-Vorstellungen auf einer Webseite.

Rust-Backend (axum + Postgres), React-Frontend. Laufzeit, Genre und
Filmplakat werden von den Kinoseiten gelesen und auf der Webseite,
in den Telegram-Alerts und im Kalender-Feed (`/showings.ics`) angezeigt.

## Lokal laufen lassen

```bash
docker compose up -d db
export DATABASE_URL=postgres://ov:ov@localhost:5432/ov
cd backend && cargo run            # http://localhost:8080
cd frontend && npm install && npm run dev   # Dev-Server mit Proxy (optional)
```

Details: [LOCAL_DEV.md](LOCAL_DEV.md).

Telegram-Bot: bei @BotFather anlegen, Token notieren. Den Bot im Kanal
@ov_linz als Administrator (Recht „Nachrichten posten") hinzufügen und
`TELEGRAM_CHAT_ID=@ov_linz` setzen.

## Tests

```bash
cd backend && cargo test           # braucht DATABASE_URL (docker compose up -d db)
cd frontend && npm test
```

## Docker

```bash
docker compose up --build          # App + Postgres, http://localhost:8080
```

## Produktion

Push auf `master` → GitHub Actions (`.github/workflows/deploy.yml`):
Tests, Image-Build (`ghcr.io/semtexkg/cinema`, native arm64), dann
Helm-Deploy (`helm/ov-watcher/`) ins k8s-Cluster:
**https://cinema.k-labs.app**

Benötigte GitHub-Secrets: `TELEGRAM_BOT_TOKEN`, `POSTGRES_PASSWORD`
(stabil halten, seedet die DB).

Cluster-Postgres inspizieren:

```bash
./dev/connectPostgres.sh                 # interaktive psql
./dev/connectPostgres.sh "SELECT ..."    # einmalige Query
```
