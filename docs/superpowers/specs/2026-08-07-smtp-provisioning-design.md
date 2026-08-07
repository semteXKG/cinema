# SMTP Provisioning — Design Spec

**Date:** 2026-08-07
**Status:** Draft

## Overview

The email magic-link login flow is fully implemented in code (backend `auth.rs`
handlers, frontend Marquee login UI) but **disabled in production** because no
SMTP provider is configured: `GET /api/auth/providers` reports `email: false`
and `POST /api/auth/email` returns `501`. This spec covers standing up an SMTP
sending solution so the magic link emails actually get delivered.

No application code changes are required. The backend already sends via
`lettre` (STARTTLS relay, auth with username/password), and the Helm chart /
deploy workflow already carry `SMTP_HOST`, `SMTP_PORT`, `SMTP_USERNAME`,
`SMTP_PASSWORD`, `SMTP_FROM` from GitHub secrets.

## Provider choice: Resend

Chosen provider: **Resend** (transactional email). Rationale:

- Free tier: 3000 emails/month — ample for low-frequency magic links.
- SMTP endpoint `smtp.resend.com:587` works with the existing `lettre` relay
  code unchanged (username `resend`, password = API key, STARTTLS on 587).
- Smallest DNS footprint of the options: one SPF TXT, one DKIM TXT, one DKIM
  CNAME.
- No self-hosting: mail from a home-cluster IP would be flagged as spam; a
  transactional provider gives reliable deliverability.

Sender address: `noreply@k-labs.app`.

## DNS records on k-labs.app

Resend provides exact values after adding the domain; the shape is:

| Type   | Host        | Value |
|--------|-------------|-------|
| SPF    | `@`         | `v=spf1 include:_spf.resend.com ~all` |
| DKIM   | `resend._domainkey` | `"p=<public key>"` (TXT) |
| DKIM   | `resend._domainkey` | `send._domainkey.resend.com.` (CNAME, fallback) |
| DMARC  | `_dmarc`    | `v=DMARC1; p=none;` (optional, recommended) |

Follow the Resend dashboard's "Verify Domain" checklist for the authoritative
records.

## GitHub secrets

Add to the repo's GitHub Actions secrets (Settings → Secrets and variables →
Actions):

| Secret          | Value |
|-----------------|-------|
| `SMTP_HOST`     | `smtp.resend.com` |
| `SMTP_PORT`     | `587` |
| `SMTP_USERNAME` | `resend` |
| `SMTP_PASSWORD` | Resend API key (read-only, e.g. `re_...`) |
| `SMTP_FROM`     | `noreply@k-labs.app` |

These are already consumed by `.github/workflows/deploy.yml` (lines 149-153)
and flowed into the ConfigMap + Secret by the Helm chart — no workflow edits.

## Deployment

After secrets are set, any push to `master` triggers the existing pipeline
(test → build → deploy). The pod picks up the new env vars and `providers`
flips to `"email": true`.

## Verification

1. `curl https://cinema.k-labs.app/api/auth/providers` → `"email": true`.
2. In the browser, open the login panel, enter a real address, submit → "sent"
   state shown.
3. Confirm the magic-link email arrives (check spam too).
4. Click the link → session cookie set → logged in, `/api/auth/me` returns the
   email.

## Error handling / notes

- `POST /api/auth/email` always returns `200 {"ok":true}` even on send failure
  (anti-enumeration); SMTP errors are logged server-side. So a missing/expired
  API key surfaces as a silent non-delivery — hence step 3 of verification is
  the real check.
- Email link expires after 15 minutes; invalid/expired links redirect to
  `/?error=invalid_token`.
- `SMTP_PASSWORD` lives in the k8s Secret (`ov-watcher-secret`) and the GitHub
  Actions secret; it is not in the ConfigMap.

## Out of scope

- Nicer (HTML) email template — current plain-text body is sufficient.
- SMTP failure alerting (e.g. notify via Telegram when a send fails).
- Rate limiting / abuse protection for the email endpoint.
