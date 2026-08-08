# Enable GitHub Login — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable GitHub SSO login on cinema.k-labs.app by creating a GitHub OAuth app and wiring its Client ID/Secret into the existing (already-implemented) GitHub auth flow.

**Architecture:** No application code changes. The backend (`sso_github`, `sso_github_callback`) and frontend GitHub button already exist; prod hides them because `GITHUB_CLIENT_ID`/`GITHUB_CLIENT_SECRET` are unset. This plan creates the OAuth app, sets the two GitHub secrets, triggers a deploy, and verifies the full login round-trip.

**Tech Stack:** GitHub OAuth Apps, GitHub Actions secrets, Helm, kubectl via SSH to the cluster node, `gh` CLI.

## Global Constraints

- Open to ANY GitHub user (no org restriction).
- OAuth app callback URL: `https://cinema.k-labs.app/api/auth/sso/github/callback`.
- OAuth app scope used by the backend: `user:email` (already in `oauth2_auth_url`, auth.rs:474).
- Secrets names (repo, GitHub Actions): `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`.
- Do NOT modify any Rust or frontend source.
- Cluster access via SSH: `ssh -o BatchMode=yes semtex@10.0.0.5` (kubectl lives there).

---

### Task 1: Create the GitHub OAuth app (human)

**Files:** none (github.com settings)

**Interfaces:**
- Produces: a GitHub OAuth app with Client ID and Client Secret, plus the human relaying both values back.

- [ ] **Step 1: Create the app**

Open https://github.com/settings/developers and click **New OAuth App**. Enter:

| Field | Value |
|-------|-------|
| Application name | `OV Cinema Linz` |
| Homepage URL | `https://cinema.k-labs.app` |
| Authorization callback URL | `https://cinema.k-labs.app/api/auth/sso/github/callback` |

Click **Register application**.

- [ ] **Step 2: Generate a client secret**

On the app's page, click **Generate a new client secret**. Copy both the **Client ID** (shown on the page) and the **Client Secret** (shown once) and share them with the controller. Do not commit them anywhere.

---

### Task 2: Set the GitHub secrets (controller)

**Files:** none (GitHub repo settings via `gh`)

**Interfaces:**
- Consumes: Client ID + Client Secret from Task 1.
- Produces: `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET` repository secrets.

- [ ] **Step 1: Set both secrets**

```bash
gh secret set GITHUB_CLIENT_ID --repo semtexkg/cinema --body "<CLIENT_ID>"
printf '%s' "<CLIENT_SECRET>" | gh secret set GITHUB_CLIENT_SECRET --repo semtexkg/cinema
```

- [ ] **Step 2: Verify the secrets are set**

```bash
gh secret list --repo semtexkg/cinema | grep GITHUB
```

Expected: `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET` appear.

---

### Task 3: Trigger deploy and verify the endpoint flips on

**Files:** none (deploys current `master`)

**Interfaces:**
- Consumes: the two secrets from Task 2.
- Produces: a pod with `GITHUB_CLIENT_ID`/`GITHUB_CLIENT_SECRET` env set; `/api/auth/providers` reporting `"github": true`.

- [ ] **Step 1: Ensure there is a commit to push**

The deploy workflow triggers on push to `master`. If `master` is in sync with `origin`, make a trivial commit first (e.g. `git commit --allow-empty -m "ci: trigger deploy for github login secrets"`) or amend the most recent commit's message. Otherwise push the pending commits.

```bash
git push origin master
```

- [ ] **Step 2: Watch the pipeline**

```bash
gh run list --repo semtexkg/cinema --limit 1 --json databaseId --jq '.[0].databaseId'
gh run watch <id> --repo semtexkg/cinema --exit-status
```

Expected: test, build, deploy all green.

- [ ] **Step 3: Verify the pod env**

```bash
ssh -o BatchMode=yes semtex@10.0.0.5 \
  'POD=$(kubectl get pod -n default -l app=ov-watcher -o jsonpath="{.items[0].metadata.name}"); kubectl exec -n default "$POD" -- env | grep -E "^GITHUB_"'
```

Expected: non-empty `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET`.

- [ ] **Step 4: Verify the providers endpoint**

```bash
curl -s https://cinema.k-labs.app/api/auth/providers
```

Expected: `{"email":true,"google":true,"apple":false,"github":true}` — `github` must be `true`.

---

### Task 4: End-to-end GitHub login verification

**Files:** none (browser test against production)

**Interfaces:**
- Consumes: the deployed pod from Task 3.
- Produces: proof that GitHub OAuth completes login and links accounts by email.

- [ ] **Step 1: Browser test**

Open https://cinema.k-labs.app, click **Sign in**, then **Sign in with GitHub**. GitHub shows the authorize screen (account name `OV Cinema Linz`). Click **Authorize**. You are redirected back to `/`; the header shows **Sign out** with your email.

- [ ] **Step 2: Confirm the session**

```bash
curl -s https://cinema.k-labs.app/api/auth/me -H "Cookie: ov_session=<cookie from browser>"
```

Expected: `{"id":N,"email":"<your github primary email>"}`.

- [ ] **Step 3: Account linking check (optional)**

If you previously logged in with the same email via Google, both providers should now map to one user id (`/api/auth/me` returns the same `id`). This validates the identity-linking model.

---

## Self-review notes

- **Spec coverage:** OAuth app creation (Task 1), secrets (Task 2), deploy + providers flip (Task 3), end-to-end login (Task 4). Open-to-all, no code changes, out-of-scope items absent.
- **Placeholder scan:** no TBD/TODO; the only dynamic values are the Client ID/Secret (from the human) and the run id / cookie (from commands).
- **Type consistency:** secret names match `deploy.yml:147-148` and `config.rs:88-89` (`GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`); callback URL matches `oauth2_auth_url` (`{base_url}/api/auth/sso/github/callback`).
