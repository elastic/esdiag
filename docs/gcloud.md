# Google Cloud Artifact Repository

Setup the Google Cloud Artificat Repository to publish container images to.

```sh
# Initialize the local gcloud tools
gcloud init
# Authenticate with your user credentials
gcloud auth login
# Setup Docker to use the artifact registry
gcloud auth configure-docker us-west2-docker.pkg.dev
```

```sh
# Retrieve registry details
gcloud artifacts repositories describe esdiag --project=elastic-customer-eng --location=us-west2
```

Reference Docs: https://cloud.google.com/artifact-registry/docs/docker/store-docker-container-images

# Build container image, tag and push to repository

Build a container image from the local machine; tag it for the artifact registry; and push the image.

```sh
docker build . --tag esdiag:latest
docker tag esdiag:latest us-west2-docker.pkg.dev/elastic-customer-eng/esdiag/esdiag:latest-arm64
docker push us-west2-docker.pkg.dev/elastic-customer-eng/esdiag/esdiag:latest-arm64
```

Build, tag and push an image all in one command. This does not make the image available to the local system.

```sh
docker build . --tag us-west2-docker.pkg.dev/elastic-customer-eng/esdiag/esdiag:latest-arm64 --push
```

> Note: when building from an Apple silicon Mac, the container image will be an Arm64 image, which is not compatible with Google Cloud Run.

# Google Cloud Build

Push the local code repo to a Google Cloud Build job. The tag and push steps are defined in `cloudbuild.yml`.

```sh
gcloud builds submit --config cloudbuild.yml .
```

# Google Cloud Run

Manually start a Google Cloud Run job. The `esdiag` job uses the `prod` tagged container and `esdiag-latest` job runs the `latest` tagged container.

```sh
gcloud run jobs execute esdiag --region=us-west2 --args="process,https://token:${auth_token}@upload.elastic.co/d/${upload_id}"
```
