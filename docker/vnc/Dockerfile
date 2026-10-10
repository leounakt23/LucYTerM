# Disposable RFB server for the live VNC suite (tests/vnc_integration.rs).
# TigerVNC's Xvnc is a real RFB server; SecurityTypes=None keeps the
# handshake passwordless as the test harness expects. The bare root
# window is fine — the suite only asserts a non-empty first frame.
FROM ubuntu:24.04

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends tigervnc-standalone-server \
    && rm -rf /var/lib/apt/lists/*

EXPOSE 5901

# - neveracceptdpi? not needed; root depth must match the canned pixel
#   format negotiation (24/32 bit default is fine).
CMD ["Xvnc", ":1", "-rfbport", "5901", "-SecurityTypes", "None", "-geometry", "1024x768", "-depth", "24", "-BlacklistTimeout", "0"]
