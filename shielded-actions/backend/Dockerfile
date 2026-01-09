# Build stage - use Debian for better Rust compilation support
FROM hexpm/elixir:1.17.1-erlang-27.1.1-debian-bookworm-20251229 AS build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    git \
    curl \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust via rustup (need nightly for edition2024)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain nightly
ENV PATH="/root/.cargo/bin:${PATH}"

# Optimize Rust compilation for limited memory environments
ENV CARGO_BUILD_JOBS=2
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV RUSTFLAGS="-C codegen-units=1"

WORKDIR /app

# Install hex and rebar
RUN mix local.hex --force && \
    mix local.rebar --force

# Set build ENV
ENV MIX_ENV=prod
# Force building native code (don't use precompiled)
ENV BUILD_NATIVE=true

# Install mix dependencies first (better caching)
COPY mix.exs mix.lock ./
RUN mix deps.get --only prod

# Create config directory
RUN mkdir config

# Copy config files
COPY config/config.exs config/prod.exs config/runtime.exs config/

# Compile dependencies separately (better error visibility)
RUN mix deps.compile

# Copy application code
COPY lib lib

# Compile the application
RUN mix compile

# Build release
RUN mix release

# Runtime stage - use slim Debian for smaller image
FROM debian:bookworm-slim AS app

RUN apt-get update && apt-get install -y \
    libstdc++6 \
    openssl \
    libncurses6 \
    locales \
    && rm -rf /var/lib/apt/lists/*

# Set locale
RUN sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen && locale-gen
ENV LANG=en_US.UTF-8
ENV LANGUAGE=en_US:en
ENV LC_ALL=en_US.UTF-8

WORKDIR /app

RUN useradd --create-home app
RUN chown app:app /app

USER app

COPY --from=build --chown=app:app /app/_build/prod/rel/backend ./

ENV HOME=/app
ENV MIX_ENV=prod
# PORT is set by Render (10000) or can be overridden
ENV PORT=10000

CMD ["bin/backend", "start"]
