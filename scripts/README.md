# Docker Build Scripts

This directory contains scripts for building and managing Docker images for the esdiag project.

## Scripts

### `docker-build.sh`

A comprehensive multi-architecture Docker build script with optimized caching and configurable options.

#### Features

- **Multi-architecture support**: Build for AMD64, ARM64, or both
- **Optimized caching**: Uses GitHub Actions cache, registry cache, or local cache
- **Environment-aware**: Automatically detects CI environments
- **Registry management**: Supports different registries for dev/prod
- **Flexible configuration**: Environment variables and command-line options

#### Quick Start

```bash
# Build for all platforms (default)
./scripts/docker-build.sh

# Build for AMD64 only (faster for testing)
./scripts/docker-build.sh --platforms linux/amd64

# Build for production
./scripts/docker-build.sh --registry-type prod --tag v1.0.0

# Local development (no push)
./scripts/docker-build.sh --no-push
```

#### Performance Optimizations

The script includes several optimizations to reduce build time:

1. **cargo-chef**: Optimal dependency caching for Rust projects
2. **BuildKit cache mounts**: Persistent cargo registry and target caches
3. **Multi-stage builds**: Separate dependency and application builds
4. **Platform-specific builders**: Optimized for multi-arch builds
5. **Smart cache selection**: Auto-selects best cache strategy for environment

#### Registry Configuration

- **Dev Registry**: `docker.elastic.co/employees`
- **Prod Registry**: `docker.elastic.co/esdiag`

#### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PLATFORMS` | `linux/amd64,linux/arm64` | Target platforms |
| `REGISTRY_TYPE` | `dev` | Registry type (dev/prod) |
| `TAG` | `dev` | Image tag |
| `BUILDER_NAME` | `esdiag-builder` | Buildx builder name |
| `DOCKERFILE` | `Dockerfile` | Dockerfile path |
| `CONTEXT` | `.` | Build context |
| `PUSH` | `true` | Push to registry |
| `CACHE_TYPE` | `auto` | Cache strategy |
| `DOCKER_USERNAME` | - | Registry username |
| `DOCKER_PASSWORD` | - | Registry password |

#### Cache Strategies

- **auto**: GHA cache in CI, local cache otherwise
- **gha**: GitHub Actions cache (CI only)
- **registry**: Registry-based cache (shared across environments)
- **local**: Local filesystem cache
- **none**: No caching

#### Examples

```bash
# Single platform for faster development
./scripts/docker-build.sh --platforms linux/amd64 --no-push

# Production release
./scripts/docker-build.sh \
  --registry-type prod \
  --tag v1.0.0 \
  --cache-type registry

# CI-style build with credentials
DOCKER_USERNAME=myuser DOCKER_PASSWORD=mypass \
./scripts/docker-build.sh --cache-type gha

# Custom builder and context
./scripts/docker-build.sh \
  --builder my-builder \
  --context /path/to/project \
  --file /path/to/Dockerfile
```

## GitHub Actions Workflows

### `container-img.yml` (Optimized)

The main CI workflow uses a matrix strategy to build each platform separately, then merges them into a multi-arch manifest. This approach provides:

- **Faster builds**: Parallel platform builds
- **Better caching**: Platform-specific caches
- **Improved reliability**: Isolated platform builds

### `docker-simple.yml` (Manual)

A simplified workflow for manual builds using the docker-build.sh script. Useful for:

- **Testing**: Quick manual builds
- **Releases**: Controlled production builds
- **Development**: Custom configurations

## Performance Notes

### Build Time Improvements

The optimizations implemented should significantly reduce build times:

1. **First build**: ~10-15 minutes (with dependency download)
2. **Cached builds**: ~3-5 minutes (dependencies cached)
3. **Code-only changes**: ~1-2 minutes (full cache hit)

### Multi-arch Considerations

- **ARM64 emulation**: Slower on AMD64 runners, but optimized with native builders
- **Dependency caching**: Shared across architectures for Rust dependencies
- **Layer caching**: Platform-specific optimizations

### Memory Usage

The builds are optimized for GitHub Actions runners:

- **Efficient layer caching**: Minimizes memory usage
- **Staged builds**: Reduces intermediate image sizes
- **Stripped binaries**: Smaller final images

## Troubleshooting

### Common Issues

1. **Cache misses**: Check cache strategy and permissions
2. **Platform failures**: Verify QEMU setup for emulation
3. **Registry auth**: Ensure credentials are properly configured
4. **Builder issues**: Try recreating the buildx builder

### Debug Mode

Enable verbose output:

```bash
DOCKER_BUILDKIT_PROGRESS=plain ./scripts/docker-build.sh
```

### Cache Debugging

Check cache usage:

```bash
docker buildx du  # Show cache usage
docker system df  # Show disk usage
```
