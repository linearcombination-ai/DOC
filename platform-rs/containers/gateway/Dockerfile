# The gateway: nginx (unprivileged: non-root, no capabilities) with DOC's two origins, 5005 (API) and 8089 (files).
#   docker build -f platform-rs/containers/gateway/Dockerfile -t doc-rs/gateway:local platform-rs/containers/gateway
FROM nginxinc/nginx-unprivileged:1.29-alpine
COPY default.conf /etc/nginx/conf.d/default.conf
EXPOSE 5005 8089
HEALTHCHECK --interval=10s --timeout=5s --start-period=5s --retries=6 \
  CMD ["sh", "-c", "wget -q -O /dev/null http://127.0.0.1:5005/gateway/health && wget -q -O /dev/null http://127.0.0.1:8089/gateway/health"]
