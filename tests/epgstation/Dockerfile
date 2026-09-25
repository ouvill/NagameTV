ARG NODE_IMAGE=node:24-trixie@sha256:be40f6a87b9b22215ddb20da0a2320a5c6d583fe3ee3b0024d9fa4f05b40c8fd
FROM ${NODE_IMAGE}
ARG SOURCE_COMMIT
WORKDIR /app
# Use the provider's locked dependencies and compile its unmodified server source.
# Git dependencies in the upstream lockfile use SSH URLs; fetch them over HTTPS.
RUN git config --global url.https://github.com/.insteadOf ssh://git@github.com/ \
    && git init . \
    && git remote add origin https://github.com/stuayu/EPGStation.git \
    && git fetch --depth=1 origin "$SOURCE_COMMIT" \
    && git checkout --detach FETCH_HEAD \
    && test "$(git rev-parse HEAD)" = "$SOURCE_COMMIT"
ENV DOCKER=YES
RUN npm ci --no-audit --no-fund && npm run compile \
    && git diff --exit-code -- src api.yml package.json package-lock.json
COPY bootstrap.cjs /app/contract-bootstrap.cjs
COPY media/ /app/recorded/
LABEL org.opencontainers.image.revision=$SOURCE_COMMIT
EXPOSE 8888
CMD ["node", "contract-bootstrap.cjs"]
