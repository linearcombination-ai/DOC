# syntax=docker/dockerfile:1.7
# The gateway: nginx with DOC's URLs and ports in front of the TypeScript build (see nginx.conf). Build context: platform-ts/docker/gateway.
FROM nginx:1.27-alpine
RUN rm -f /etc/nginx/conf.d/* && mkdir -p /srv/content
COPY nginx.conf /etc/nginx/nginx.conf
# nginx syntax is checked when the image is built (the host that develops this has no nginx)
RUN nginx -t
EXPOSE 5005 8089
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=5 CMD ["wget", "-q", "-O", "/dev/null", "http://127.0.0.1:5005/gateway-health"]
CMD ["nginx", "-g", "daemon off;"]
