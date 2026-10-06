# syntax=docker/dockerfile:1
# Repository root is the build context. Intended final path: crates/convt-worker/deploy/railway/Dockerfile.
# Deployment requires the parent task's --sandbox-child / Landlock refactor.
FROM rust:1.95.0-bookworm AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends patchelf \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY packages/db/migrations/ packages/db/migrations/
RUN cargo build --locked --release -p convt-worker \
    && patchelf --force-rpath --set-rpath '$ORIGIN/lib' target/release/convt-worker
# Fixed binary RPATH provides the bundled C++/codec closure for dlopen without
# leaking LD_LIBRARY_PATH to system LibreOffice subprocesses.

FROM ubuntu:24.04 AS runtime
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
       ca-certificates coreutils dash python3 patchelf fontconfig fonts-dejavu-core fonts-liberation \
       libreoffice-writer libreoffice-calc libreoffice-impress \
    && rm -rf /var/lib/apt/lists/*
# These are existing audited Linux x86_64 artifacts, never rebuilt by this image.
COPY packaging/out/convt/ffmpeg packaging/out/convt/ffprobe /opt/convt/
COPY packaging/out/convt/lib/ /opt/convt/lib/
COPY packaging/out/convt/licenses/ /opt/convt/licenses/
COPY --from=build /build/target/release/convt-worker /opt/convt/convt-worker
COPY crates/convt-worker/deploy/railway/entrypoint.sh /usr/local/bin/convt-worker-entrypoint
RUN chmod 0755 /usr/local/bin/convt-worker-entrypoint \
    && test "$(dpkg --print-architecture)" = amd64 \
    && test -x /opt/convt/ffmpeg && test -x /opt/convt/ffprobe \
    && test -f /opt/convt/lib/libpdfium.so \
    && test -f /opt/convt/lib/libheif.so.1 \
    && test -d /opt/convt/lib/libheif/plugins \
    && test -x /usr/bin/soffice \
    && patchelf --force-rpath --set-rpath /usr/lib/libreoffice/program /usr/lib/libreoffice/program/soffice.bin \
    && ln -s /usr/lib/libreoffice/program/soffice.bin /opt/convt/soffice \
    && mkdir -p /srv/convt-template/opt /srv/convt-template/usr /srv/convt-template/etc \
       /srv/convt-template/dev /srv/convt-template/var \
    && cp -al /opt/convt /srv/convt-template/opt/ \
    && cp -al /usr/bin /usr/lib /usr/lib64 /usr/share /srv/convt-template/usr/ \
    && cp -a /lib /lib64 /srv/convt-template/ \
    && ln -s usr/bin /srv/convt-template/bin \
    && cp -a /etc/fonts /etc/libreoffice /etc/alternatives /srv/convt-template/etc/ \
    && cp -a /etc/ld.so.cache /srv/convt-template/etc/ \
    && printf 'root:x:0:0:root:/work:/bin/sh\n' > /srv/convt-template/etc/passwd \
    && printf 'root:x:0:\n' > /srv/convt-template/etc/group \
    && printf 'passwd: files\ngroup: files\n' > /srv/convt-template/etc/nsswitch.conf \
    && ln -s /output/scratch /srv/convt-template/var/tmp \
    && find /srv/convt-template -type f -exec chmod a-w {} + \
    && find /srv/convt-template -type d -exec chmod a-w {} +
# /srv/convt-template is a selected trusted template, not the container root. The
# parent finalizes isolated roots, job-owned /output and per-job passwd entries.
# No /proc, credentials, network configuration, Docker client or socket inside it.
ENV PATH=/opt/convt:/usr/bin:/bin \
    CONVT_SANDBOX_TEMPLATE=/srv/convt-template
# Root parent needs chroot, setuid/setgid and mknod; child drops privilege.
# Dockerfile USER does not grant capabilities or enable Landlock on Railway.
USER 0:0
WORKDIR /opt/convt
ENTRYPOINT ["/usr/local/bin/convt-worker-entrypoint"]
