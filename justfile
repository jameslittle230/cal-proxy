# Registry URL for container images
REGISTRY := "container-registry.gotcha.ninja"
PROJECT := "cal-proxy"

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
    docker build -t {{REGISTRY}}/{{PROJECT}}:latest .

# Build and run the Docker image locally
docker-run: build
    docker run -p 3000:3000 {{REGISTRY}}/{{PROJECT}}:latest

# Login to GitHub Container Registry, build and publish the image
publish: build
    echo "Logging into GitHub Container Registry..."
    @echo "Please enter your GitHub Personal Access Token when prompted:"
    docker login {{REGISTRY}}
    docker push {{REGISTRY}}/{{PROJECT}}:latest
    echo "Successfully pushed image to {{REGISTRY}}/{{PROJECT}}:latest"

# Build and publish with a specific version tag
publish-version version: build
    docker tag {{REGISTRY}}/{{PROJECT}}:latest {{REGISTRY}}/{{PROJECT}}:{{version}}
    docker push {{REGISTRY}}/{{PROJECT}}:{{version}}
    docker push {{REGISTRY}}/{{PROJECT}}:latest
    echo "Successfully pushed image to {{REGISTRY}}/{{PROJECT}}:{{version}}"

# Clean up build artifacts
clean:
    cargo clean
    rm -rf target/
