ARG RUST_VERSION=1.72.0
FROM rust:${RUST_VERSION}-slim-bullseye AS build

WORKDIR /app
COPY . /app

RUN cargo build --release && \
    cp ./target/release/serv /bin/server

FROM debian:bullseye-slim
COPY --from=build /bin/server /bin/
CMD ["/bin/server"]