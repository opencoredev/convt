FROM ubuntu:24.04
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends libreoffice-writer libreoffice-calc libreoffice-impress python3 ca-certificates && rm -rf /var/lib/apt/lists/*
COPY payload /opt/convt
ENV PATH=/opt/convt:/usr/bin:/bin HOME=/work XDG_CONFIG_HOME=/work/config XDG_CACHE_HOME=/work/cache CONVT_LICENSE_STORE=file
USER 10001:10001
WORKDIR /work
ENTRYPOINT ["/opt/convt/convt"]
