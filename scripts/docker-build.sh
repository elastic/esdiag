#!/bin/bash

# Docker Multi-Arch Build Script for esdiag
# Supports both local development and CI/CD environments
# Optimized for fast builds with proper caching
set -euo pipefail

# Default configuration
DEFAULT_PLATFORMS="linux/amd64,linux/arm64"
DEFAULT_DEV_REGISTRY="docker.elastic.co/employees"
DEFAULT_PROD_REGISTRY="docker.elastic.co"
DEFAULT_TAG="dev"
DEFAULT_BUILDER_NAME="esdiag-builder"

# Detect available CPU cores for optimized builds
CPU_CORES=$(nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo "4")
MAX_PARALLEL_BUILDS="${MAX_PARALLEL_BUILDS:-$CPU_CORES}"

# Environment variables with defaults
PLATFORMS="${PLATFORMS:-$DEFAULT_PLATFORMS}"
REGISTRY_TYPE="${REGISTRY_TYPE:-dev}"  # dev or prod
TAG="${TAG:-$DEFAULT_TAG}"
BUILDER_NAME="${BUILDER_NAME:-$DEFAULT_BUILDER_NAME}"
DOCKERFILE="${DOCKERFILE:-Dockerfile}"
CONTEXT="${CONTEXT:-.}"
PUSH="${PUSH:-true}"
CACHE_TYPE="${CACHE_TYPE:-auto}"  # auto, gha, registry, local, or none

# Docker credentials (can be set via environment or passed as arguments)
DOCKER_USERNAME="${DOCKER_USERNAME:-}"
DOCKER_PASSWORD="${DOCKER_PASSWORD:-}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Help function
show_help() {
    cat << EOF
Docker Multi-Arch Build Script for esdiag

USAGE:
    $0 [OPTIONS]

OPTIONS:
    -p, --platforms PLATFORMS    Target platforms (default: linux/amd64,linux/arm64)
                                Use 'linux/amd64' or 'linux/arm64' for single platform
    -r, --registry-type TYPE     Registry type: dev or prod (default: dev)
    -t, --tag TAG               Image tag (default: dev)
    -b, --builder NAME          Buildx builder name (default: esdiag-builder)
    -f, --file DOCKERFILE       Dockerfile path (default: Dockerfile)
    -c, --context CONTEXT       Build context (default: .)
    --no-push                   Build only, don't push to registry
    --cache-type TYPE           Cache type: auto, gha, registry, local, none (default: auto)
    --username USERNAME         Docker registry username
    --password PASSWORD         Docker registry password
    -h, --help                  Show this help message

ENVIRONMENT VARIABLES:
    PLATFORMS                   Target platforms
    REGISTRY_TYPE              Registry type (dev/prod)
    TAG                        Image tag
    BUILDER_NAME               Buildx builder name
    DOCKERFILE                 Dockerfile path
    CONTEXT                    Build context
    PUSH                       Push to registry (true/false)
    CACHE_TYPE                 Cache strategy
    MAX_PARALLEL_BUILDS        Maximum parallel builds (auto-detected)
    DOCKER_USERNAME            Registry username
    DOCKER_PASSWORD            Registry password

EXAMPLES:
    # Build for all platforms (dev registry)
    $0

    # Build for amd64 only
    $0 --platforms linux/amd64

    # Build for production registry
    $0 --registry-type prod --tag v1.0.0

    # Local build without pushing
    $0 --no-push

    # Use specific cache type
    $0 --cache-type registry

    # Build with custom credentials
    $0 --username myuser --password mypass

REGISTRY CONFIGURATION:
    dev:  $DEFAULT_DEV_REGISTRY
    prod: $DEFAULT_PROD_REGISTRY
EOF
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -p|--platforms)
                PLATFORMS="$2"
                shift 2
                ;;
            -r|--registry-type)
                REGISTRY_TYPE="$2"
                shift 2
                ;;
            -t|--tag)
                TAG="$2"
                shift 2
                ;;
            -b|--builder)
                BUILDER_NAME="$2"
                shift 2
                ;;
            -f|--file)
                DOCKERFILE="$2"
                shift 2
                ;;
            -c|--context)
                CONTEXT="$2"
                shift 2
                ;;
            --no-push)
                PUSH="false"
                shift
                ;;
            --cache-type)
                CACHE_TYPE="$2"
                shift 2
                ;;
            --username)
                DOCKER_USERNAME="$2"
                shift 2
                ;;
            --password)
                DOCKER_PASSWORD="$2"
                shift 2
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
}

# Determine registry URL
get_registry() {
    case "$REGISTRY_TYPE" in
        dev)
            echo "$DEFAULT_DEV_REGISTRY"
            ;;
        prod)
            echo "$DEFAULT_PROD_REGISTRY"
            ;;
        *)
            log_error "Invalid registry type: $REGISTRY_TYPE (must be 'dev' or 'prod')"
            exit 1
            ;;
    esac
}

# Determine cache configuration
get_cache_config() {
    local cache_from=""
    local cache_to=""
    
    case "$CACHE_TYPE" in
        auto)
            # Use GHA cache if in GitHub Actions, otherwise local
            if [[ "${GITHUB_ACTIONS:-false}" == "true" ]]; then
                cache_from="type=gha"
                cache_to="type=gha,mode=max"
            else
                cache_from="type=local,src=/tmp/.buildx-cache"
                cache_to="type=local,dest=/tmp/.buildx-cache,mode=max"
            fi
            ;;
        gha)
            cache_from="type=gha"
            cache_to="type=gha,mode=max"
            ;;
        registry)
            local registry=$(get_registry)
            cache_from="type=registry,ref=${registry}/esdiag:buildcache"
            cache_to="type=registry,ref=${registry}/esdiag:buildcache,mode=max"
            ;;
        local)
            cache_from="type=local,src=/tmp/.buildx-cache"
            cache_to="type=local,dest=/tmp/.buildx-cache,mode=max"
            ;;
        none)
            cache_from=""
            cache_to=""
            ;;
        *)
            log_error "Invalid cache type: $CACHE_TYPE"
            exit 1
            ;;
    esac
    
    echo "$cache_from|$cache_to"
}

# Check if buildx builder exists and is running
check_builder() {
    if docker buildx inspect "$BUILDER_NAME" >/dev/null 2>&1; then
        log_info "Using existing builder: $BUILDER_NAME"
        return 0
    else
        return 1
    fi
}

# Create and start buildx builder
setup_builder() {
    log_info "Creating buildx builder: $BUILDER_NAME"
    
    # Create builder with optimizations for multi-arch builds
    docker buildx create \
        --name "$BUILDER_NAME" \
        --driver docker-container \
        --driver-opt network=host \
        --bootstrap \
        --use
    
    log_success "Builder $BUILDER_NAME created and set as current"
}

# Docker login
docker_login() {
    local registry=$(get_registry)
    
    if [[ -z "$DOCKER_USERNAME" || -z "$DOCKER_PASSWORD" ]]; then
        log_warning "Docker credentials not provided, skipping login"
        log_warning "Registry: $registry"
        return 0
    fi
    
    log_info "Logging into Docker registry: $registry"
    echo "$DOCKER_PASSWORD" | docker login "$registry" --username "$DOCKER_USERNAME" --password-stdin
    log_success "Successfully logged into $registry"
}

# Build and optionally push the Docker image
build_image() {
    local registry=$(get_registry)
    local image_name="${registry}/adrianchen-es/esdiag:${TAG}"
    local cache_config=$(get_cache_config)
    local cache_from=$(echo "$cache_config" | cut -d'|' -f1)
    local cache_to=$(echo "$cache_config" | cut -d'|' -f2)
    
    log_info "Building Docker image..."
    log_info "Platforms: $PLATFORMS"
    log_info "Image: $image_name"
    log_info "Push: $PUSH"
    log_info "Cache strategy: $CACHE_TYPE"
    log_info "Available CPU cores: $CPU_CORES"
    log_info "Max parallel builds: $MAX_PARALLEL_BUILDS"
    
    # Build command arguments
    local build_args=(
        "buildx" "build"
        "--platform" "$PLATFORMS"
        "--file" "$DOCKERFILE"
        "--tag" "$image_name"
        "--build-arg" "BUILDKIT_INLINE_CACHE=1"
    )
    
    # Add cache configuration if specified
    if [[ -n "$cache_from" ]]; then
        build_args+=("--cache-from" "$cache_from")
    fi
    if [[ -n "$cache_to" ]]; then
        build_args+=("--cache-to" "$cache_to")
    fi
    
    # Add push flag
    if [[ "$PUSH" == "true" ]]; then
        build_args+=("--push")
    else
        build_args+=("--load")
    fi
    
    # Add context
    build_args+=("$CONTEXT")
    
    # Execute build
    docker "${build_args[@]}"
    
    if [[ "$PUSH" == "true" ]]; then
        log_success "Successfully built and pushed: $image_name"
    else
        log_success "Successfully built: $image_name"
    fi
}

# Cleanup function
cleanup() {
    if [[ "${CLEANUP_BUILDER:-false}" == "true" ]]; then
        log_info "Cleaning up builder: $BUILDER_NAME"
        docker buildx rm "$BUILDER_NAME" >/dev/null 2>&1 || true
    fi
}

# Main execution
main() {
    log_info "Starting Docker multi-arch build process"
    
    # Parse arguments
    parse_args "$@"
    
    # Validate prerequisites
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed or not in PATH"
        exit 1
    fi
    
    # Setup cleanup trap
    trap cleanup EXIT
    
    # Setup builder
    if ! check_builder; then
        setup_builder
        CLEANUP_BUILDER=true
    fi
    
    # Login to registry (if credentials provided)
    if [[ "$PUSH" == "true" ]]; then
        docker_login
    fi
    
    # Create cache directory for local builds
    if [[ "$CACHE_TYPE" == "local" || ("$CACHE_TYPE" == "auto" && "${GITHUB_ACTIONS:-false}" != "true") ]]; then
        mkdir -p /tmp/.buildx-cache
    fi
    
    # Build image
    build_image
    
    log_success "Build process completed successfully!"
}

# Execute main function with all arguments
main "$@"
