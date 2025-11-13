# Registry URL for container images
REGISTRY := "registry.int.69salem.com"
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
docker-build:
    docker build -t {{REGISTRY}}/{{PROJECT}}:latest .

# Build and run the Docker image locally
docker-run: docker-build
    docker run -p 3000:3000 {{REGISTRY}}/{{PROJECT}}:latest

# Login to GitHub Container Registry, build and publish the image
docker-publish version: docker-build
    docker login {{REGISTRY}}

    docker tag {{REGISTRY}}/{{PROJECT}}:latest {{REGISTRY}}/{{PROJECT}}:{{version}}
    docker push {{REGISTRY}}/{{PROJECT}}:{{version}}
    echo "Successfully pushed image to {{REGISTRY}}/{{PROJECT}}:{{version}}"
    docker push {{REGISTRY}}/{{PROJECT}}:latest
    echo "Successfully pushed image to {{REGISTRY}}/{{PROJECT}}:latest"


# Clean up build artifacts
clean:
    cargo clean
    rm -rf target/
