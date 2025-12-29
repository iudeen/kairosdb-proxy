# Versioning and Release Guide

This project follows [Semantic Versioning 2.0.0](https://semver.org/) (semver).

## Semantic Versioning Overview

Given a version number **MAJOR.MINOR.PATCH**, increment the:

- **MAJOR** version when you make incompatible API changes
- **MINOR** version when you add functionality in a backward compatible manner
- **PATCH** version when you make backward compatible bug fixes

Additional labels for pre-release and build metadata are available as extensions to the MAJOR.MINOR.PATCH format.

## Version Increment Guidelines

### When to increment MAJOR version (breaking changes)

- Changing the configuration file format in a non-backward-compatible way
- Removing or renaming configuration options
- Changing API endpoints or request/response formats
- Removing support for features or backends
- Changes that require users to modify their existing configurations or deployments

**Example:** `0.1.0` → `1.0.0`

### When to increment MINOR version (new features)

- Adding new configuration options (with sensible defaults)
- Adding new endpoints or functionality
- Adding new proxy modes or routing strategies
- Performance improvements or optimizations
- New features that don't break existing functionality

**Example:** `0.1.0` → `0.2.0`

### When to increment PATCH version (bug fixes)

- Bug fixes that don't change functionality
- Security patches
- Documentation updates
- Internal refactoring without behavior changes
- Dependency updates (unless they introduce new features)

**Example:** `0.1.0` → `0.1.1`

## Creating a New Release

Follow these steps to create a new release:

### 1. Update the version number

Update the version in `kairos-proxy/Cargo.toml`:

```toml
[package]
name = "kairos-proxy"
version = "X.Y.Z"  # Update this line
edition = "2021"
```

### 2. Update Cargo.lock

Run the following command to update `Cargo.lock`:

```bash
cargo build --manifest-path kairos-proxy/Cargo.toml
```

### 3. Commit the version change

```bash
git add kairos-proxy/Cargo.toml Cargo.lock
git commit -m "chore: bump version to X.Y.Z"
git push origin main
```

### 4. Create and push a git tag

```bash
# Create an annotated tag
git tag -a vX.Y.Z -m "Release version X.Y.Z"

# Push the tag to GitHub
git push origin vX.Y.Z
```

**Note:** The tag must follow the format `vX.Y.Z` (e.g., `v0.2.0`, `v1.0.0`) for the release workflow to trigger.

### 5. Automated release process

Once the tag is pushed, GitHub Actions will automatically:

1. Build the Docker image
2. Tag the image with:
   - The version number (`X.Y.Z`)
   - The full version tag (`vX.Y.Z`)
   - `latest` (for the most recent release)
3. Push the image to GitHub Container Registry at `ghcr.io/iudeen/kairosdb-proxy`
4. Create a GitHub release with auto-generated release notes

### 6. Verify the release

After the workflow completes:

1. Check the [GitHub Releases page](https://github.com/iudeen/kairosdb-proxy/releases) for the new release
2. Verify the Docker image is available at `ghcr.io/iudeen/kairosdb-proxy:vX.Y.Z`
3. Pull and test the image:

```bash
docker pull ghcr.io/iudeen/kairosdb-proxy:vX.Y.Z
docker run --rm ghcr.io/iudeen/kairosdb-proxy:vX.Y.Z --version
```

## Release Checklist

Before creating a release, ensure:

- [ ] All tests pass (`cargo test --manifest-path kairos-proxy/Cargo.toml`)
- [ ] Code is properly formatted (`cargo fmt --all -- --check`)
- [ ] No clippy warnings (`cargo clippy --all -- -D warnings`)
- [ ] Documentation is up to date
- [ ] CHANGELOG or release notes are prepared (if applicable)
- [ ] Version number is updated in `kairos-proxy/Cargo.toml`
- [ ] `Cargo.lock` is updated
- [ ] Version increment follows semver guidelines

## Using Released Images

### Pull the latest version

```bash
docker pull ghcr.io/iudeen/kairosdb-proxy:latest
```

### Pull a specific version

```bash
docker pull ghcr.io/iudeen/kairosdb-proxy:v0.2.0
```

### Use in Docker Compose

```yaml
services:
  kairos-proxy:
    image: ghcr.io/iudeen/kairosdb-proxy:v0.2.0
    ports:
      - "8080:8080"
    environment:
      - KAIROS_PROXY_CONFIG=/app/config.toml
    volumes:
      - ./config.toml:/app/config.toml
```

### Use in Kubernetes/Helm

Update the `image.repository` and `image.tag` in your Helm values:

```yaml
image:
  repository: ghcr.io/iudeen/kairosdb-proxy
  tag: v0.2.0
  pullPolicy: IfNotPresent
```

## Pre-release Versions

For alpha, beta, or release candidate versions, append the pre-release identifier:

- Alpha: `v0.2.0-alpha.1`
- Beta: `v0.2.0-beta.1`
- Release Candidate: `v0.2.0-rc.1`

Pre-release versions are useful for testing before a stable release.

## Troubleshooting

### Workflow doesn't trigger

- Ensure the tag follows the `vX.Y.Z` format (starts with `v`)
- Check that the tag was pushed to the repository (`git push origin vX.Y.Z`)
- Verify GitHub Actions are enabled for the repository

### Docker image push fails

- Ensure the repository has the `GITHUB_TOKEN` secret available (automatically provided)
- Check that the workflow has permissions to write packages
- Verify the GHCR permissions in repository settings

### Image not appearing in GHCR

- Check the workflow logs in the Actions tab
- Ensure the package is set to public in GHCR settings
- Wait a few minutes for the image to become available

## Additional Resources

- [Semantic Versioning Specification](https://semver.org/)
- [GitHub Container Registry Documentation](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry)
- [GitHub Actions - Publishing Docker Images](https://docs.github.com/en/actions/publishing-packages/publishing-docker-images)
