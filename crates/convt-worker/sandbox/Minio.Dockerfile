FROM golang:1.25.14-alpine AS build
WORKDIR /source
COPY source/ .
RUN CGO_ENABLED=0 go build -trimpath -ldflags='-s -w' -o /minio .
FROM ubuntu:24.04
COPY --from=build /minio /usr/local/bin/minio
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/minio"]
