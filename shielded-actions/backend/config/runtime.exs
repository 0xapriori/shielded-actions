import Config

# Runtime configuration - loaded at runtime (not compile time)
# This is where we read environment variables

if config_env() == :prod do
  # Get port from environment, default to 8080
  port = String.to_integer(System.get_env("PORT") || "8080")

  config :backend,
    port: port
end
