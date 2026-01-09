import Config

# General application configuration
config :backend,
  generators: [timestamp_type: :utc_datetime]

# Import environment specific config
import_config "#{config_env()}.exs"
