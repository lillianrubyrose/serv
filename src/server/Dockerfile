ARG RUST_VERSION=1.74.0
FROM rust:${RUST_VERSION}-slim-bullseye AS build

WORKDIR /app
COPY . /app

RUN cargo build --release --bin=serv-server && \
    cp ./target/release/serv-server /bin/server

FROM debian:bullseye-slim
COPY --from=build /bin/server /bin/
CMD ["/bin/server"]