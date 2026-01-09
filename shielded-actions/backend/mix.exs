defmodule Backend.MixProject do
  use Mix.Project

  def project do
    [
      app: :backend,
      version: "0.1.0",
      elixir: "~> 1.17",
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      releases: [
        backend: [
          include_executables_for: [:unix]
        ]
      ]
    ]
  end

  def application do
    [
      extra_applications: [:logger],
      mod: {Backend.Application, []}
    ]
  end

  defp deps do
    [
      # Web server
      {:plug_cowboy, "~> 2.7"},
      {:plug, "~> 1.16"},

      # JSON encoding/decoding
      {:jason, "~> 1.4"},

      # CORS support
      {:corsica, "~> 2.1"},

      # HTTP client
      {:req, "~> 0.5"},

      # Ethereum interaction
      {:ethereumex, "~> 0.10"},
      {:ex_keccak, "~> 0.7"},
      {:ex_rlp, "~> 0.6"},
      {:ex_secp256k1, "~> 0.7"},

      # Anoma SDK (for proof generation)
      {:anoma_sdk, github: "anoma/anoma-sdk", branch: "main"},

      # Environment variables
      {:dotenvy, "~> 0.9"}
    ]
  end
end
