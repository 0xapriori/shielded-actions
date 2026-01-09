defmodule Backend.Application do
  @moduledoc """
  Application module for the Shielded Actions backend.
  Starts the web server and other supervised processes.
  """

  use Application

  @impl true
  def start(_type, _args) do
    # Load environment variables from .env file if present
    Dotenvy.source([".env", System.get_env()])

    port = String.to_integer(System.get_env("PORT") || "4000")

    children = [
      # Start the Cowboy web server
      {Plug.Cowboy, scheme: :http, plug: Backend.Router, options: [port: port]},
      # Start the resource store for tracking shielded resources
      {Backend.ResourceStore, []}
    ]

    opts = [strategy: :one_for_one, name: Backend.Supervisor]

    IO.puts("Starting Shielded Actions backend on port #{port}")
    IO.puts("API endpoints available at http://localhost:#{port}/api")

    Supervisor.start_link(children, opts)
  end
end
