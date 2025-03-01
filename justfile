# Default recipe to run when just is called without arguments
default:
    @just --list

# Run the development server
dev:
    cargo run

# Run with auto-reload on file changes
dev-watch:
    cargo watch -x run

# Build the Docker image
build:
    docker build -t ghcr.io/jameslittle230/cal-proxy:latest .

# Build and run the Docker image locally
docker-run: build
    docker run -p 3000:3000 ghcr.io/jameslittle230/cal-proxy:latest

# Login to GitHub Container Registry, build and publish the image
publish: build
    echo "Logging into GitHub Container Registry..."
    @echo "Please enter your GitHub Personal Access Token when prompted:"
    docker login ghcr.io -u jameslittle230
    docker push ghcr.io/jameslittle230/cal-proxy:latest
    echo "Successfully pushed image to ghcr.io/jameslittle230/cal-proxy:latest"

# Build and publish with a specific version tag
publish-version version: build
    docker tag ghcr.io/jameslittle230/cal-proxy:latest ghcr.io/jameslittle230/cal-proxy:{{version}}
    docker push ghcr.io/jameslittle230/cal-proxy:{{version}}
    docker push ghcr.io/jameslittle230/cal-proxy:latest
    echo "Successfully pushed image to ghcr.io/jameslittle230/cal-proxy:{{version}}"

# Clean up build artifacts
clean:
    cargo clean
    rm -rf target/