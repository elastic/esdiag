# Google Cloud Artifact Repository

Setup the Google Cloud Artifact Repository to publish container images to.
```sh
# Initialize the local gcloud tools
gcloud init
# Authenticate with your user credentials
gcloud auth login
# Setup Docker to use the artifact registry
gcloud auth configure-docker ${region}-docker.pkg.dev
```

```sh
# Retrieve registry details
gcloud artifacts repositories describe esdiag --project=${project} --location=${region}
```

Reference Docs: https://cloud.google.com/artifact-registry/docs/docker/store-docker-container-images
