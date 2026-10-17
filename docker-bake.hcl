# The API image is always built for both platforms, as one multi-platform
# image under one tag. The architecture is never part of the tag: Docker
# picks the right variant when the image is pulled.
#
#   docker buildx bake                                  build only
#   docker buildx bake --load                           into the local image store
#   IMAGE=ghcr.io/<user>/todo-dx-api TAG=1.0.0 docker buildx bake --push

variable "IMAGE" {
  default = "todo-dx-api"
}

variable "TAG" {
  default = "latest"
}

group "default" {
  targets = ["api"]
}

# CI replaces the tags and labels of this target (docker/metadata-action).
target "docker-metadata-action" {
  tags = ["${IMAGE}:${TAG}"]
}

target "api" {
  inherits   = ["docker-metadata-action"]
  context    = "."
  dockerfile = "backend.Dockerfile"
  platforms  = ["linux/amd64", "linux/arm64"]
}
