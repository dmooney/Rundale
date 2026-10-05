#!/usr/bin/env bash
# Deploy limerick-bug-report and work its private report inbox in limerick-prod.
#
#   limerick-prod.sh deploy        # create the bucket if needed; build and deploy
#   limerick-prod.sh url           # print the service URL
#   limerick-prod.sh list          # report IDs waiting in the inbox
#   limerick-prod.sh pull <dir>    # download every waiting report into <dir>/<id>/,
#                                  # then delete those reports from the bucket
#
# Reports never leave limerick-prod except through `pull`. The bucket blocks
# public access; only the service (to write) and the operator (to pull) reach it.
set -euo pipefail

project=limerick-prod
region=us-east1
service=limerick-bug-report
runtime_account="$service-runtime@$project.iam.gserviceaccount.com"
build_account="limerick-build@$project.iam.gserviceaccount.com"
bucket="gs://$project-bug-reports"
# The iPhone app's Firebase app (the one limerick-endpoints binds).
app_id=1:877612517009:ios:586f98a2cc3e7d0c676130
source_dir="$(cd "$(dirname "$0")/.." && pwd)"
# The Artifact Registry repository limerick-endpoints' images live in.
image_repo="$region-docker.pkg.dev/$project/limerick/$service"

usage() {
    sed -n '4,8p' "$0" | sed 's/^# //' >&2
    exit 2
}

ensure_infrastructure() {
    if ! gcloud storage buckets describe "$bucket" --project "$project" >/dev/null 2>&1; then
        gcloud storage buckets create "$bucket" --project "$project" --location "$region" \
            --uniform-bucket-level-access --public-access-prevention >/dev/null
    fi
    if ! gcloud iam service-accounts describe "$runtime_account" --project "$project" >/dev/null 2>&1; then
        gcloud iam service-accounts create "$service-runtime" --project "$project" \
            --display-name "limerick-bug-report runtime" >/dev/null
    fi
    # A new service account takes a few seconds to reach IAM.
    local granted=false
    for _ in 1 2 3 4 5 6; do
        if gcloud projects add-iam-policy-binding "$project" --member "serviceAccount:$runtime_account" \
            --role roles/logging.logWriter --condition=None >/dev/null 2>&1; then
            granted=true
            break
        fi
        sleep 10
    done
    if [[ $granted != true ]]; then
        echo "limerick-prod.sh: could not grant $runtime_account roles/logging.logWriter" >&2
        exit 1
    fi
    # verifyIdToken(checkRevoked) reads the Firebase user.
    gcloud projects add-iam-policy-binding "$project" --member "serviceAccount:$runtime_account" \
        --role roles/firebaseauth.viewer --condition=None >/dev/null
    # Object access on this bucket only: create-only writes answer 412 for a resend.
    gcloud storage buckets add-iam-policy-binding "$bucket" \
        --member "serviceAccount:$runtime_account" --role roles/storage.objectUser >/dev/null
}

# An empty inbox is not an error.
waiting_ids() {
    { gcloud storage ls "$bucket/inbox/*/report.json" 2>/dev/null || true; } \
        | sed -E 's#.*/inbox/([^/]+)/report.json#\1#'
}

case "${1:-}" in
    deploy)
        ensure_infrastructure
        image="$image_repo:$(date -u +%Y%m%d%H%M%S)"
        # Built as limerick-build, the account limerick-endpoints' images use.
        gcloud builds submit "$source_dir" --project "$project" --region "$region" \
            --config "$source_dir/deploy/cloudbuild.yaml" --substitutions "_IMAGE=$image" \
            --service-account "projects/$project/serviceAccounts/$build_account" --quiet
        # One instance keeps the in-memory hourly limit whole.
        gcloud run deploy "$service" --project "$project" --region "$region" \
            --image "$image" --service-account "$runtime_account" \
            --allow-unauthenticated --max-instances 1 --memory 512Mi \
            --set-env-vars "FIREBASE_PROJECT_ID=$project,ALLOWED_APP_IDS=$app_id,REPORT_BUCKET=${bucket#gs://}" \
            --quiet
        ;;
    url)
        gcloud run services describe "$service" --project "$project" --region "$region" \
            --format 'value(status.url)'
        ;;
    list)
        waiting_ids
        ;;
    pull)
        [[ $# -eq 2 ]] || usage
        destination="$2"
        mkdir -p "$destination"
        for id in $(waiting_ids); do
            gcloud storage cp --recursive "$bucket/inbox/$id" "$destination/" >/dev/null
            # Delete only what arrived intact.
            if [[ -s "$destination/$id/report.json" ]]; then
                gcloud storage rm --recursive "$bucket/inbox/$id" >/dev/null
                echo "$destination/$id"
            else
                echo "limerick-prod.sh: $id did not download; left in the inbox" >&2
            fi
        done
        ;;
    *) usage ;;
esac
