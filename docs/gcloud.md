# Google Cloud Artifact Repository

Setup the Google Cloud Artifact Repository to publish container images to.
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

Use the `/bin/gcp/local-build.sh` script. It uses `buildx` to build multi-arch images for both `amd64` and `arm64` architectures.
