# SMTP Provisioning for Email Magic-Link Login — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up Resend as the SMTP provider for `cinema.k-labs.app` so the already-implemented email magic-link login actually sends mail.

**Architecture:** No application code changes. The backend (`auth.rs` `post_email`) already sends via `lettre` (STARTTLS relay + username/password), and the Helm chart + deploy workflow already carry `SMTP_HOST/PORT/USERNAME/PASSWORD/FROM` from GitHub secrets. This plan creates the Resend account/domain, adds DNS records, sets the GitHub secrets, and triggers a redeploy so `/api/auth/providers` flips `email` to true.

**Tech Stack:** Resend (transactional email), k-labs.app DNS, GitHub Actions secrets, Helm, kubectl, `gh` CLI.

## Global Constraints

- Sender address must be `noreply@k-labs.app`.
- Resend SMTP endpoint: `smtp.resend.com:587`, username `resend`, password = API key.
- SMTP username in config is `SMTP_USERNAME=resend`; API key is `SMTP_PASSWORD` (never committed, only GitHub secret).
- DNS values below match Resend's dashboard; if the dashboard shows different values, the dashboard wins.
- Do not modify any Rust or frontend source. No `backend/` or `frontend/` file changes.

---

### Task 1: Create Resend account, API key, and add k-labs.app domain

**Files:** none (browser + clipboard work)

**Interfaces:**
- Produces: Resend API key string (`re_...`) and DNS record values (SPF/DKIM/DMARC) from Resend's domain dashboard.

- [ ] **Step 1: Create the Resend account**

Open https://resend.com and sign up (email + password, or Google OAuth). Confirm the signup email. No payment method required for the free tier (3000 emails/month).

- [ ] **Step 2: Create a read-only API key**

In the Resend dashboard go to **API Keys → Create API Key**, name it `cinema-magic-link`, permission **Sending access only** (or `Full access` if the dashboard only offers that), and copy the generated `re_...` value into a local scratch file (`/tmp/resend-key.txt`). It is only shown once.

Verify the key works:

```bash
curl -s https://api.resend.com/domains -H "Authorization: Bearer $(cat /tmp/resend-key.txt)"
```

Expected: HTTP 200 and a JSON body (likely `{"data":[]}`).

- [ ] **Step 3: Add the k-labs.app domain**

In the dashboard go to **Domains → Add Domain**, enter `k-labs.app`, region default (`us-east-1`). Resend shows three DNS records to add (SPF TXT, DKIM TXT, DKIM CNAME) plus an optional DMARC suggestion. **Copy these exact values** into `/tmp/resend-dns.txt` (do not close the dialog yet — or reopen later under Domains → k-labs.app).

Keep the `/tmp/resend-key.txt` and `/tmp/resend-dns.txt` files; the next tasks consume them. If you closed the dialog, the records are still visible under **Domains → k-labs.app** (the DKIM value may be shown as a TXT record on Resend's current dashboard; follow what it shows).

---

### Task 2: Add DNS records to k-labs.app

**Files:** none (DNS provider's admin UI — whichever registrar hosts `k-labs.app`)

**Interfaces:**
- Consumes: DNS values from Task 1 (`/tmp/resend-dns.txt`).
- Produces: live SPF/DKIM/DMARC records for `k-labs.app`, visible to `dig` from anywhere.

- [ ] **Step 1: Add the records**

Log in to the DNS provider for `k-labs.app` and add the records from Task 1. The shape is:

| Type | Name | Value |
|------|------|-------|
| TXT | `@` | `v=spf1 include:_spf.resend.com ~all` |
| TXT | `resend._domainkey` | `"p=<long base64 key from Resend>"` |
| CNAME | `resend._domainkey` | `send._domainkey.resend.com` (if Resend shows a CNAME instead of the TXT) |
| TXT | `_dmarc` | `v=DMARC1; p=none;` (optional) |

Note: `dig` is not installed locally; the GitHub Actions runner or Resend's dashboard check is the verification path.

- [ ] **Step 2: Verify the SPF record**

Resend's dashboard will poll and mark the domain **Verified** once the records propagate (usually minutes to a few hours). Refresh the Domains page until the domain status is **Verified**.

If it doesn't verify within a few hours, check each record against Resend's dashboard exactly (SPF `~all` vs `-all`, DKIM TXT vs CNAME variant).

---

### Task 3: Set GitHub Actions secrets

**Files:** none (repo Settings UI)

**Interfaces:**
- Consumes: `SMTP_PASSWORD` from `/tmp/resend-key.txt`.
- Produces: 5 GitHub secrets consumed by `.github/workflows/deploy.yml:149-153`.

- [ ] **Step 1: Add the secrets**

Open https://github.com/semtexkg/cinema/settings/secrets/actions and add each as a **repository** secret:

| Secret | Value |
|--------|-------|
| `SMTP_HOST` | `smtp.resend.com` |
| `SMTP_PORT` | `587` |
| `SMTP_USERNAME` | `resend` |
| `SMTP_PASSWORD` | contents of `/tmp/resend-key.txt` |
| `SMTP_FROM` | `noreply@k-labs.app` |

- [ ] **Step 2: Verify the secrets are set**

```bash
gh secret list --repo semtexkg/cinema
```

Expected: the five `SMTP_*` names appear in the list (values are hidden).

- [ ] **Step 3: Clean up scratch files**

```bash
rm -f /tmp/resend-key.txt /tmp/resend-dns.txt
```

---

### Task 4: Trigger redeploy and verify email is enabled

**Files:** none (deploys current `master`)

**Interfaces:**
- Consumes: the 5 GitHub secrets from Task 3.
- Produces: a running pod with `SMTP_*` env vars set, `/api/auth/providers` reporting `"email": true`.

The deploy workflow triggers on push to `master`. The design-spec commit is already on `master` locally and unpushed; pushing it triggers the pipeline. If `master` is already in sync, make a trivial push instead (e.g. amend the spec commit message) — or run the workflow manually if you prefer.

- [ ] **Step 1: Push master to trigger the pipeline**

```bash
git push origin master
```

Then watch the run:

```bash
gh run watch --repo semtexkg/cinema
```

Expected: all three jobs (test → build → deploy) go green. The deploy job passes `--set config.smtpHost=smtp.resend.com` etc. from the new secrets.

- [ ] **Step 2: Confirm the pod has SMTP env vars**

```bash
kubectl get pod -n default -l app=ov-watcher -o jsonpath='{.items[0].metadata.name}'
POD=$(kubectl get pod -n default -l app=ov-watcher -o jsonpath='{.items[0].metadata.name}')
kubectl exec -n default "$POD" -- env | grep '^SMTP_'
```

Expected: `SMTP_HOST=smtp.resend.com`, `SMTP_PORT=587`, `SMTP_USERNAME=resend`, `SMTP_FROM=noreply@k-labs.app`, and a non-empty `SMTP_PASSWORD`.

- [ ] **Step 3: Verify the providers endpoint**

```bash
curl -s https://cinema.k-labs.app/api/auth/providers
```

Expected: `{"email":true,"google":...,"apple":...,"github":...}` — the `email` field must be `true` (was `false` before).

---

### Task 5: End-to-end magic-link verification

**Files:** none (browser test against production)

**Interfaces:**
- Consumes: the deployed pod from Task 4.
- Produces: proof that a magic-link email is delivered and completes login.

- [ ] **Step 1: Request a magic link**

Open https://cinema.k-labs.app, click the **Sign in** button in the header, enter a real email address you control in the email field, click **Send link**. The button should flip to the "sent" state.

- [ ] **Step 2: Confirm delivery**

Check the inbox (and spam folder) for an email from `noreply@k-labs.app` with subject **OV-Kino Linz — Sign in** and a `https://cinema.k-labs.app/api/auth/verify?token=...` link. If it arrives within a minute or two, SMTP is working.

If nothing arrives within ~5 minutes, check the pod logs for a lettre error (the endpoint always returns 200, so logs are the only signal):

```bash
kubectl logs -n default deploy/ov-watcher --tail=200 | grep -i "send email\|smtp"
```

Expected: no `send email failed` lines.

- [ ] **Step 3: Complete the login**

Click the link. You should be redirected to `/`, the header should now show **Sign out** with your email, and `GET /api/auth/me` should return your email.

```bash
curl -s https://cinema.k-labs.app/api/auth/me -H "Cookie: ov_session=<cookie from browser>"
```

- [ ] **Step 4: Commit any follow-up fix**

If the delivery failed and a backend fix was required, commit it. Otherwise no commit needed — the plan introduces no source changes.

---

## Self-review notes

- Spec coverage: Resend choice ✓ (Task 1), DNS records ✓ (Task 2), GitHub secrets ✓ (Task 3), deployment ✓ (Task 4), verification incl. spam-check + 15-min expiry note ✓ (Task 5). Out-of-scope items (HTML template, alerting, rate limiting) are intentionally absent.
- Placeholder scan: no TBD/TODO; the only dynamic values are Resend-provided and explicitly marked as "dashboard wins".
- Type consistency: env var names match config.rs (`SMTP_HOST`, `SMTP_PORT`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `SMTP_FROM`) and deploy.yml exactly.
