# Disposable RDP server for RDP verification. xrdp terminates the
# protocol; openbox gives the session something to start, so the client
# gets past TLS/NLS auth into a real (minimal) session.
FROM ubuntu:24.04

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        xrdp openbox \
    && useradd -m -s /bin/bash test \
    && echo 'test:test' | chpasswd \
    && rm -rf /var/lib/apt/lists/*

# xrdp reads its config from /etc/xrdp; the package default (TLS with
# self-signed cert) is what the client verifies against (/cert handling
# stays strict on the app side).
EXPOSE 3389

CMD ["xrdp", "--nodaemon"]
