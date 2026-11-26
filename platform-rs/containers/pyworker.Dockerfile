# syntax=docker/dockerfile:1.7
#
# The Python docx and PDF worker: the seam's other side (python/seam_server.py), with Prince and WeasyPrint.
# Same base image, same system packages, same Prince .deb and the same extra fonts as DOC's own Dockerfile, so PDFs
# come out of the same engine with the same fonts; the Calibre/ePub parts are not here (ePub is out of scope).
# Build context: the repository root.
#
#   docker build -f platform-rs/containers/pyworker.Dockerfile -t doc-rs/pyworker:local .
#
# Licence note (the captain handles it; see docs/COMPARISON.md): the Prince .deb is the one DOC installs and runs
# unlicensed-for-commercial-use; its output carries Prince's notice on the first page. WeasyPrint (BSD) is the fallback:
# the service switches to it when Prince is missing or fails on a document.

FROM python:3.13-slim-bookworm

ARG PRINCE_VERSION=16.1-1
ARG FONTS_REPO=https://github.com/Bible-Translation-Tools/ScriptureAppBuilder-pipeline

RUN apt-get update && apt-get install -y --no-install-recommends \
    wget ca-certificates git fontconfig \
    pango1.0-tools libpango-1.0-0 libpangoft2-1.0-0 libharfbuzz-subset0 \
    texlive-fonts-recommended \
    fonts-khmeros fonts-lklug-sinhala fonts-tlwg-garuda-otf fonts-lohit-orya fonts-lohit-mlym fonts-lohit-knda \
    fonts-lohit-telu fonts-lohit-taml fonts-lohit-gujr fonts-lohit-guru fonts-lohit-beng-bengali fonts-lohit-deva \
    fonts-noto fonts-noto-cjk fonts-dzongkha fonts-tibetan-machine fonts-baekmuk fonts-ipafont-mincho \
    fonts-arphic-uming fonts-opensymbol fonts-liberation2 \
    libaom3 libavif15 libgif7 libjpeg62-turbo liblcms2-2 libtiff6 libwebp7 libwebpdemux2 \
 && rm -rf /var/lib/apt/lists/*

# Prince, installed the way DOC's Dockerfile installs it (the .deb from princexml.com; no licence file, so Prince runs in its non-commercial mode and puts
# its notice on the first page, which is DOC's situation today). DOC's Dockerfile names the amd64 package only; this one asks for the package of the
# image's own architecture and does not fail the build when there is none: the service then uses WeasyPrint and says so in a finding.
RUN set -eu; \
    arch="$(dpkg --print-architecture)"; \
    deb="prince_${PRINCE_VERSION}_debian12_${arch}.deb"; \
    if wget -nv -O "/tmp/${deb}" "https://www.princexml.com/download/${deb}"; then \
      apt-get update; apt-get install -y --no-install-recommends "/tmp/${deb}"; \
      rm -f "/tmp/${deb}"; rm -rf /var/lib/apt/lists/*; \
      prince --version; \
    else \
      echo "WARNING: Prince ${PRINCE_VERSION} is not available for ${arch}; PDF output will use WeasyPrint" >&2; \
    fi

# The extra fonts DOC adds from the Scripture App Builder pipeline repository, then refresh the font cache.
RUN git clone --depth 1 "${FONTS_REPO}" /tmp/ScriptureAppBuilder-pipeline \
 && cp /tmp/ScriptureAppBuilder-pipeline/ContainerImage/home/fonts/*.ttf /usr/share/fonts/ \
 && rm -rf /tmp/ScriptureAppBuilder-pipeline \
 && fc-cache -f

# WeasyPrint at DOC's pinned versions (backend/requirements.txt as a constraints file; pip refuses extras such as `fonttools[woff]` there).
COPY backend/requirements.txt /tmp/doc-requirements.txt
RUN python -m venv /opt/venv \
 && sed -E 's/\[[^]]*\]//' /tmp/doc-requirements.txt > /tmp/constraints.txt \
 && /opt/venv/bin/pip install --no-cache-dir --upgrade pip \
 && /opt/venv/bin/pip install --no-cache-dir -c /tmp/constraints.txt weasyprint \
 && rm /tmp/doc-requirements.txt /tmp/constraints.txt
ENV PATH=/opt/venv/bin:${PATH}

RUN groupadd --system --gid 10001 doc \
 && useradd --system --uid 10001 --gid 10001 --home-dir /app --shell /usr/sbin/nologin doc \
 && mkdir -p /app/document_output /app/python \
 && chown -R doc:doc /app
COPY --chown=doc:doc platform-rs/python /app/python

ENV PYWORKER_HOST=0.0.0.0 \
    PYWORKER_PORT=9100 \
    PYWORKER_CONCURRENCY=2 \
    PYWORKER_ROOT=/app/document_output \
    PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1
WORKDIR /app
USER 10001:10001
EXPOSE 9100
HEALTHCHECK --interval=10s --timeout=10s --start-period=20s --retries=6 \
  CMD ["python", "-c", "import urllib.request; urllib.request.urlopen('http://127.0.0.1:9100/readyz', timeout=8)"]
STOPSIGNAL SIGTERM
ENTRYPOINT ["python", "-I", "/app/python/seam_server.py"]
