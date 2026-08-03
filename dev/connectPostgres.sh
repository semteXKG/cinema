#!/usr/bin/env bash
set -euo pipefail

# Connect to the Postgres running inside the k8s cluster.
#
# This dev box has no kubectl/psql, so the script SSHes to a node that
# does and execs into the postgres pod there.
#
# Usage:
#   ./dev/connectPostgres.sh                 interactive psql shell
#   ./dev/connectPostgres.sh "SELECT 1"      run one query, then exit
#
# Overrides via env:
#   NODE=user@host   SSH target with kubectl + cluster access (default: semtex@10.0.0.5)
#   NAMESPACE=       k8s namespace            (default: default)
#   POD=             postgres pod name        (default: ov-watcher-postgres-0)
#   DB_USER= DB_NAME=                         (defaults: ov / ov)

NODE="${NODE:-semtex@10.0.0.5}"
NAMESPACE="${NAMESPACE:-default}"
POD="${POD:-ov-watcher-postgres-0}"
DB_USER="${DB_USER:-ov}"
DB_NAME="${DB_NAME:-ov}"

# Password stays base64 over the wire (safe charset), decoded inside the pod.
PW_B64="$(ssh -o BatchMode=yes "$NODE" \
  "kubectl get secret ov-watcher-secret -n $NAMESPACE -o jsonpath='{.data.POSTGRES_PASSWORD}'")"

exec_in_pod() {
  # $1 = ssh TTY flag ("" or "-t"), $2 = kubectl exec TTY flag ("-i" or "-it")
  local ssh_flags=(-o BatchMode=yes)
  [ -n "$1" ] && ssh_flags+=(-t)
  local pod_cmd="kubectl exec $2 -n $NAMESPACE $POD -- sh -c 'PGPASSWORD=\$(echo \"$PW_B64\" | base64 -d) exec psql -U $DB_USER -d $DB_NAME'"
  ssh "${ssh_flags[@]}" "$NODE" "$pod_cmd"
}

if [ $# -eq 0 ]; then
  exec_in_pod "-t" "-it"
else
  printf '%s\n' "$*" | exec_in_pod "" "-i"
fi
