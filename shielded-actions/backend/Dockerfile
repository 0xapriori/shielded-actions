# Build stage
FROM hexpm/elixir:1.17.3-erlang-27.1.2-alpine-3.20.3 AS build

# Install build dependencies
RUN apk add --no-cache build-base git curl

# Install Rust via rustup (need nightly for edition2024)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain nightly
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /app

# Install hex and rebar
RUN mix local.hex --force && \
    mix local.rebar --force

# Set build ENV
ENV MIX_ENV=prod
# Force building native code (don't use precompiled)
ENV BUILD_NATIVE=true

# Install mix dependencies
COPY mix.exs mix.lock ./
RUN mix deps.get --only prod
RUN mkdir config

# Copy config files
COPY config/config.exs config/prod.exs config/runtime.exs config/
RUN mix deps.compile

# Copy application code
COPY lib lib

# Compile the application
RUN mix compile

# Build release
RUN mix release

# Runtime stage
FROM alpine:3.20.3 AS app

RUN apk add --no-cache libstdc++ openssl ncurses-libs

WORKDIR /app

RUN chown nobody:nobody /app

USER nobody:nobody

COPY --from=build --chown=nobody:nobody /app/_build/prod/rel/backend ./

ENV HOME=/app
ENV MIX_ENV=prod
# PORT is set by Render (10000) or can be overridden
ENV PORT=10000

CMD ["bin/backend", "start"]
