#!/usr/bin/env bash
# One-time setup letting .github/workflows/deploy-limerick-prod.yml deploy to
# limerick-prod without a stored key: a Workload Identity Federation provider
# trusting only that workflow on dmooney/Rundale's main branch in the
# limerick-prod environment, and a deploy service account with the roles
# `limerick-prod.sh deploy` and `definitions` need.
# Run it as a limerick-prod owner; it is safe to run again.
set -euo pipefail

project=limerick-prod
repo=dmooney/Rundale
pool=github
provider=rundale
deployer=limerick-deployer
deployer_email="$deployer@$project.iam.gserviceaccount.com"
runtime_email="limerick-endpoints-runtime@$project.iam.gserviceaccount.com"
build_email="limerick-build@$project.iam.gserviceaccount.com"
number="$(gcloud projects describe "$project" --format='value(projectNumber)')"

gcloud iam workload-identity-pools describe "$pool" --project "$project" --location global \
    >/dev/null 2>&1 \
    || gcloud iam workload-identity-pools create "$pool" --project "$project" --location global \
        --display-name "GitHub Actions"
# Only the deploy workflow on main, in the limerick-prod environment, may sign
# in: any other workflow job on main can also request a GitHub token.
# Running the script again applies this to an existing provider.
workflow="$repo/.github/workflows/deploy-limerick-prod.yml@refs/heads/main"
action=create-oidc
gcloud iam workload-identity-pools providers describe "$provider" --project "$project" \
    --location global --workload-identity-pool "$pool" >/dev/null 2>&1 && action=update-oidc
mapping="google.subject=assertion.sub,attribute.repository=assertion.repository,attribute.ref=assertion.ref"
mapping+=",attribute.workflow_ref=assertion.job_workflow_ref,attribute.environment=assertion.environment"
oidc_args=(--project "$project" --location global --workload-identity-pool "$pool"
    --attribute-mapping "$mapping"
    --attribute-condition "assertion.repository=='$repo' && assertion.ref=='refs/heads/main' && assertion.job_workflow_ref=='$workflow' && assertion.environment=='limerick-prod'")
[[ $action == create-oidc ]] && oidc_args+=(--issuer-uri https://token.actions.githubusercontent.com)
gcloud iam workload-identity-pools providers "$action" "$provider" "${oidc_args[@]}"

gcloud iam service-accounts describe "$deployer_email" --project "$project" >/dev/null 2>&1 \
    || gcloud iam service-accounts create "$deployer" --project "$project" \
        --display-name "GitHub Actions deploys"
gcloud iam service-accounts add-iam-policy-binding "$deployer_email" --project "$project" \
    --role roles/iam.workloadIdentityUser \
    --member "principalSet://iam.googleapis.com/projects/$number/locations/global/workloadIdentityPools/$pool/attribute.repository/$repo" \
    >/dev/null

# Build, read the image digest, update the migration job and the service, and
# reach the database through the Auth Proxy for `definitions`.
for role in roles/cloudbuild.builds.editor roles/artifactregistry.reader roles/run.developer \
    roles/logging.viewer roles/cloudsql.client roles/serviceusage.serviceUsageConsumer; do
    gcloud projects add-iam-policy-binding "$project" --member "serviceAccount:$deployer_email" \
        --role "$role" --condition None >/dev/null
done
# Run builds as limerick-build and deploy Cloud Run as the runtime identity.
for account in "$build_email" "$runtime_email"; do
    gcloud iam service-accounts add-iam-policy-binding "$account" --project "$project" \
        --member "serviceAccount:$deployer_email" --role roles/iam.serviceAccountUser >/dev/null
done
gcloud storage buckets add-iam-policy-binding "gs://${project}_cloudbuild" \
    --member "serviceAccount:$deployer_email" --role roles/storage.objectAdmin >/dev/null
gcloud secrets add-iam-policy-binding DATABASE_URL --project "$project" \
    --member "serviceAccount:$deployer_email" --role roles/secretmanager.secretAccessor >/dev/null
# The server reads the TypeSafe key at runtime.
gcloud secrets add-iam-policy-binding TYPESAFE_API_KEY --project "$project" \
    --member "serviceAccount:$runtime_email" --role roles/secretmanager.secretAccessor >/dev/null

gh variable set GCP_WORKLOAD_IDENTITY_PROVIDER --repo "$repo" \
    --body "projects/$number/locations/global/workloadIdentityPools/$pool/providers/$provider"
gh variable set GCP_DEPLOY_SERVICE_ACCOUNT --repo "$repo" --body "$deployer_email"
echo "GitHub deploys to $project are set up."
