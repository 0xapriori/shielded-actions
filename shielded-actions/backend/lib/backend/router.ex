defmodule Backend.Router do
  @moduledoc """
  HTTP Router for the Shielded Actions backend API.
  Provides endpoints for shield, swap, and unshield operations.
  """

  use Plug.Router
  use Plug.ErrorHandler

  alias Backend.ProofService
  alias Backend.ResourceStore

  # CORS support for frontend
  plug(Corsica,
    origins: "*",
    allow_headers: ["content-type", "authorization"],
    allow_methods: ["GET", "POST", "OPTIONS"]
  )

  plug(Plug.Logger)
  plug(:match)

  plug(Plug.Parsers,
    parsers: [:json],
    pass: ["application/json"],
    json_decoder: Jason
  )

  plug(:dispatch)

  # Health check endpoint
  get "/health" do
    send_resp(conn, 200, Jason.encode!(%{status: "ok", service: "shielded-actions-backend"}))
  end

  # API Info endpoint
  get "/api/info" do
    info = %{
      version: "0.1.0",
      network: "sepolia",
      contracts: %{
        protocol_adapter: "0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525",
        weth_forwarder: "0xD5307D777dC60b763b74945BF5A42ba93ce44e4b",
        usdc_forwarder: "0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE",
        uniswap_forwarder: "0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA",
        weth: "0x7b79995e5f793A07Bc00c21412e50Ecae098E7f9",
        usdc: "0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238"
      },
      endpoints: [
        "POST /api/shield - Create shield transaction proof",
        "POST /api/swap - Create shielded swap transaction proof",
        "POST /api/unshield - Create unshield transaction proof",
        "GET /api/resources/:address - Get shielded resources for address"
      ]
    }

    send_resp(conn, 200, Jason.encode!(info))
  end

  # Shield endpoint - converts ERC20 tokens to shielded resources
  post "/api/shield" do
    case conn.body_params do
      %{
        "token" => token,
        "amount" => amount,
        "sender" => sender,
        "nullifier_key" => nullifier_key
      } ->
        case ProofService.create_shield_transaction(token, amount, sender, nullifier_key) do
          {:ok, result} ->
            # Store the resource for tracking
            ResourceStore.store_resource(result.resource_commitment, %{
              owner: sender,
              token: token,
              amount: amount,
              resource: result.resource,
              commitment: result.resource_commitment
            })

            send_resp(conn, 200, Jason.encode!(result))

          {:error, reason} ->
            send_resp(conn, 400, Jason.encode!(%{error: reason}))
        end

      _ ->
        send_resp(
          conn,
          400,
          Jason.encode!(%{error: "Missing required fields: token, amount, sender, nullifier_key"})
        )
    end
  end

  # Swap endpoint - creates a shielded swap transaction
  post "/api/swap" do
    case conn.body_params do
      %{
        "input_resource" => input_resource,
        "output_token" => output_token,
        "nullifier_key" => nullifier_key,
        "min_amount_out" => min_amount_out
      } ->
        case ProofService.create_swap_transaction(
               input_resource,
               output_token,
               nullifier_key,
               min_amount_out
             ) do
          {:ok, result} ->
            send_resp(conn, 200, Jason.encode!(result))

          {:error, reason} ->
            send_resp(conn, 400, Jason.encode!(%{error: reason}))
        end

      _ ->
        send_resp(
          conn,
          400,
          Jason.encode!(%{
            error: "Missing required fields: input_resource, output_token, nullifier_key, min_amount_out"
          })
        )
    end
  end

  # Unshield endpoint - converts shielded resources back to ERC20 tokens
  post "/api/unshield" do
    case conn.body_params do
      %{
        "resource" => resource,
        "recipient" => recipient,
        "nullifier_key" => nullifier_key
      } ->
        case ProofService.create_unshield_transaction(resource, recipient, nullifier_key) do
          {:ok, result} ->
            send_resp(conn, 200, Jason.encode!(result))

          {:error, reason} ->
            send_resp(conn, 400, Jason.encode!(%{error: reason}))
        end

      _ ->
        send_resp(
          conn,
          400,
          Jason.encode!(%{error: "Missing required fields: resource, recipient, nullifier_key"})
        )
    end
  end

  # Get shielded resources for an address
  get "/api/resources/:address" do
    resources = ResourceStore.get_resources_by_owner(address)
    send_resp(conn, 200, Jason.encode!(%{address: address, resources: resources}))
  end

  # Generate a new nullifier key pair
  post "/api/generate-keypair" do
    case ProofService.generate_keypair() do
      {:ok, keypair} ->
        send_resp(conn, 200, Jason.encode!(keypair))

      {:error, reason} ->
        send_resp(conn, 500, Jason.encode!(%{error: reason}))
    end
  end

  # Catch-all for unmatched routes
  match _ do
    send_resp(conn, 404, Jason.encode!(%{error: "Not found"}))
  end

  defp handle_errors(conn, %{kind: _kind, reason: _reason, stack: _stack}) do
    send_resp(conn, conn.status, Jason.encode!(%{error: "Internal server error"}))
  end
end
