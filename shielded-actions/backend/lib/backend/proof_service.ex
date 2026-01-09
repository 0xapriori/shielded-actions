defmodule Backend.ProofService do
  @moduledoc """
  Service for generating Anoma Resource Machine proofs.

  Can operate in two modes:
  1. Mock mode (default) - returns placeholder proofs for testing
  2. Prover mode - calls the Rust prover service for real ZK proofs

  Set PROVER_URL environment variable to enable prover mode.
  """

  require Logger

  # Contract addresses on Sepolia
  @contracts %{
    protocol_adapter: "0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525",
    weth_forwarder: "0xD5307D777dC60b763b74945BF5A42ba93ce44e4b",
    usdc_forwarder: "0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE",
    uniswap_forwarder: "0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA",
    weth: "0x7b79995e5f793A07Bc00c21412e50Ecae098E7f9",
    usdc: "0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238"
  }

  # Token decimals
  @token_decimals %{
    "WETH" => 18,
    "USDC" => 6
  }

  # Get the prover service URL from environment
  defp prover_url do
    System.get_env("PROVER_URL")
  end

  defp use_real_prover? do
    prover_url() != nil
  end

  @doc """
  Generate a new nullifier key pair for a user.
  Returns both the private key and the commitment.
  """
  @spec generate_keypair() :: {:ok, map()} | {:error, String.t()}
  def generate_keypair do
    # Generate random 32-byte keys
    private_key = :crypto.strong_rand_bytes(32)
    public_key = :crypto.hash(:sha256, private_key)

    {:ok,
     %{
       private_key: Base.encode16(private_key, case: :lower),
       public_key: Base.encode16(public_key, case: :lower)
     }}
  end

  @doc """
  Create a shield transaction that converts ERC20 tokens to shielded resources.
  """
  @spec create_shield_transaction(String.t(), String.t(), String.t(), String.t()) ::
          {:ok, map()} | {:error, String.t()}
  def create_shield_transaction(token, amount, sender, nullifier_key_hex) do
    if use_real_prover?() do
      create_shield_transaction_with_prover(token, amount, sender, nullifier_key_hex)
    else
      create_shield_transaction_mock(token, amount, sender, nullifier_key_hex)
    end
  end

  defp create_shield_transaction_with_prover(token, amount, sender, nullifier_key_hex) do
    try do
      body = %{
        token: token,
        amount: amount,
        sender: sender,
        nullifier_key: nullifier_key_hex
      }

      case Req.post("#{prover_url()}/api/prove/shield", json: body) do
        {:ok, %{status: 200, body: proof_response}} ->
          # Build the full response with proof and forwarder call
          forwarder_address = get_forwarder_address(token)
          decimals = Map.get(@token_decimals, String.upcase(token), 18)
          amount_wei = parse_amount(amount, decimals)

          resource = build_resource(token, amount_wei, sender, nullifier_key_hex, forwarder_address)
          resource_commitment = :crypto.hash(:sha256, :erlang.term_to_binary(resource))

          {:ok,
           %{
             transaction: Jason.encode!(proof_response),
             resource_commitment: Base.encode16(resource_commitment, case: :lower),
             resource: resource,
             forwarder_call: %{
               to: forwarder_address,
               data: encode_shield_call(sender, amount_wei)
             },
             proof: proof_response
           }}

        {:ok, %{status: status, body: body}} ->
          {:error, "Prover error (#{status}): #{inspect(body)}"}

        {:error, reason} ->
          Logger.warning("Prover service unavailable, falling back to mock: #{inspect(reason)}")
          create_shield_transaction_mock(token, amount, sender, nullifier_key_hex)
      end
    rescue
      e ->
        Logger.error("Prover call failed: #{inspect(e)}")
        create_shield_transaction_mock(token, amount, sender, nullifier_key_hex)
    end
  end

  defp create_shield_transaction_mock(token, amount, sender, nullifier_key_hex) do
    try do
      nullifier_key = decode_hex(nullifier_key_hex)
      forwarder_address = get_forwarder_address(token)
      decimals = Map.get(@token_decimals, String.upcase(token), 18)
      amount_wei = parse_amount(amount, decimals)

      resource = build_resource(token, amount_wei, sender, nullifier_key_hex, forwarder_address)
      resource_commitment = :crypto.hash(:sha256, :erlang.term_to_binary(resource))
      mock_proof = generate_mock_proof()

      {:ok,
       %{
         transaction: Jason.encode!(%{
           actions: [%{compliance_units: [mock_proof]}],
           delta_proof: mock_proof,
           mock: true,
           note: "Mock proof. Set PROVER_URL for real ZK proofs via Bonsai/Boundless."
         }),
         resource_commitment: Base.encode16(resource_commitment, case: :lower),
         resource: resource,
         forwarder_call: %{
           to: forwarder_address,
           data: encode_shield_call(sender, amount_wei)
         }
       }}
    rescue
      e ->
        {:error, "Failed to create shield transaction: #{inspect(e)}"}
    end
  end

  @doc """
  Create a shielded swap transaction.
  """
  @spec create_swap_transaction(map(), String.t(), String.t(), String.t()) ::
          {:ok, map()} | {:error, String.t()}
  def create_swap_transaction(input_resource_map, output_token, nullifier_key_hex, min_amount_out) do
    if use_real_prover?() do
      create_swap_transaction_with_prover(input_resource_map, output_token, nullifier_key_hex, min_amount_out)
    else
      create_swap_transaction_mock(input_resource_map, output_token, nullifier_key_hex, min_amount_out)
    end
  end

  defp create_swap_transaction_with_prover(input_resource_map, output_token, nullifier_key_hex, min_amount_out) do
    try do
      body = %{
        input_resource: input_resource_map,
        output_token: output_token,
        nullifier_key: nullifier_key_hex,
        min_amount_out: min_amount_out
      }

      case Req.post("#{prover_url()}/api/prove/swap", json: body) do
        {:ok, %{status: 200, body: proof_response}} ->
          nullifier_key = decode_hex(nullifier_key_hex)
          output_decimals = Map.get(@token_decimals, String.upcase(output_token), 18)
          min_amount_wei = parse_amount(min_amount_out, output_decimals)
          input_amount = input_resource_map["quantity"] || input_resource_map[:quantity] || 0
          output_forwarder = get_forwarder_address(output_token)

          output_resource = build_swap_resource(output_token, min_amount_wei, input_resource_map, nullifier_key_hex, output_forwarder)
          nullifier = :crypto.hash(:sha256, :erlang.term_to_binary({input_resource_map, nullifier_key}))
          resource_commitment = :crypto.hash(:sha256, :erlang.term_to_binary(output_resource))

          {:ok,
           %{
             transaction: Jason.encode!(proof_response),
             nullifier: Base.encode16(nullifier, case: :lower),
             new_resource_commitment: Base.encode16(resource_commitment, case: :lower),
             new_resource: output_resource,
             uniswap_call: %{
               to: @contracts.uniswap_forwarder,
               data: encode_swap_call(input_amount, min_amount_wei, output_token)
             },
             proof: proof_response
           }}

        {:ok, %{status: status, body: body}} ->
          {:error, "Prover error (#{status}): #{inspect(body)}"}

        {:error, reason} ->
          Logger.warning("Prover unavailable, falling back to mock: #{inspect(reason)}")
          create_swap_transaction_mock(input_resource_map, output_token, nullifier_key_hex, min_amount_out)
      end
    rescue
      e ->
        Logger.error("Prover call failed: #{inspect(e)}")
        create_swap_transaction_mock(input_resource_map, output_token, nullifier_key_hex, min_amount_out)
    end
  end

  defp create_swap_transaction_mock(input_resource_map, output_token, nullifier_key_hex, min_amount_out) do
    try do
      nullifier_key = decode_hex(nullifier_key_hex)
      output_decimals = Map.get(@token_decimals, String.upcase(output_token), 18)
      min_amount_wei = parse_amount(min_amount_out, output_decimals)
      input_amount = input_resource_map["quantity"] || input_resource_map[:quantity] || 0
      output_forwarder = get_forwarder_address(output_token)

      output_resource = build_swap_resource(output_token, min_amount_wei, input_resource_map, nullifier_key_hex, output_forwarder)
      nullifier = :crypto.hash(:sha256, :erlang.term_to_binary({input_resource_map, nullifier_key}))
      resource_commitment = :crypto.hash(:sha256, :erlang.term_to_binary(output_resource))
      mock_proof = generate_mock_proof()

      {:ok,
       %{
         transaction: Jason.encode!(%{
           actions: [%{compliance_units: [mock_proof]}],
           delta_proof: mock_proof,
           mock: true,
           note: "Mock proof. Set PROVER_URL for real ZK proofs via Bonsai/Boundless."
         }),
         nullifier: Base.encode16(nullifier, case: :lower),
         new_resource_commitment: Base.encode16(resource_commitment, case: :lower),
         new_resource: output_resource,
         uniswap_call: %{
           to: @contracts.uniswap_forwarder,
           data: encode_swap_call(input_amount, min_amount_wei, output_token)
         }
       }}
    rescue
      e ->
        {:error, "Failed to create swap transaction: #{inspect(e)}"}
    end
  end

  @doc """
  Create an unshield transaction that converts shielded resources back to ERC20 tokens.
  """
  @spec create_unshield_transaction(map(), String.t(), String.t()) ::
          {:ok, map()} | {:error, String.t()}
  def create_unshield_transaction(resource_map, recipient, nullifier_key_hex) do
    if use_real_prover?() do
      create_unshield_transaction_with_prover(resource_map, recipient, nullifier_key_hex)
    else
      create_unshield_transaction_mock(resource_map, recipient, nullifier_key_hex)
    end
  end

  defp create_unshield_transaction_with_prover(resource_map, recipient, nullifier_key_hex) do
    try do
      body = %{
        resource: resource_map,
        recipient: recipient,
        nullifier_key: nullifier_key_hex
      }

      case Req.post("#{prover_url()}/api/prove/unshield", json: body) do
        {:ok, %{status: 200, body: proof_response}} ->
          nullifier_key = decode_hex(nullifier_key_hex)
          amount = resource_map["quantity"] || resource_map[:quantity] || 0
          nullifier = :crypto.hash(:sha256, :erlang.term_to_binary({resource_map, nullifier_key}))
          logic_ref = resource_map["logic_ref"] || resource_map[:logic_ref]
          forwarder_address = decode_forwarder_from_logic_ref(logic_ref)

          {:ok,
           %{
             transaction: Jason.encode!(proof_response),
             nullifier: Base.encode16(nullifier, case: :lower),
             forwarder_call: %{
               to: forwarder_address,
               data: encode_unshield_call(recipient, amount)
             },
             proof: proof_response
           }}

        {:ok, %{status: status, body: body}} ->
          {:error, "Prover error (#{status}): #{inspect(body)}"}

        {:error, reason} ->
          Logger.warning("Prover unavailable, falling back to mock: #{inspect(reason)}")
          create_unshield_transaction_mock(resource_map, recipient, nullifier_key_hex)
      end
    rescue
      e ->
        Logger.error("Prover call failed: #{inspect(e)}")
        create_unshield_transaction_mock(resource_map, recipient, nullifier_key_hex)
    end
  end

  defp create_unshield_transaction_mock(resource_map, recipient, nullifier_key_hex) do
    try do
      nullifier_key = decode_hex(nullifier_key_hex)
      amount = resource_map["quantity"] || resource_map[:quantity] || 0
      nullifier = :crypto.hash(:sha256, :erlang.term_to_binary({resource_map, nullifier_key}))
      logic_ref = resource_map["logic_ref"] || resource_map[:logic_ref]
      forwarder_address = decode_forwarder_from_logic_ref(logic_ref)
      mock_proof = generate_mock_proof()

      {:ok,
       %{
         transaction: Jason.encode!(%{
           actions: [%{compliance_units: [mock_proof]}],
           delta_proof: mock_proof,
           mock: true,
           note: "Mock proof. Set PROVER_URL for real ZK proofs via Bonsai/Boundless."
         }),
         nullifier: Base.encode16(nullifier, case: :lower),
         forwarder_call: %{
           to: forwarder_address,
           data: encode_unshield_call(recipient, amount)
         }
       }}
    rescue
      e ->
        {:error, "Failed to create unshield transaction: #{inspect(e)}"}
    end
  end

  @doc """
  Get shielded resources for an address (placeholder for demo).
  """
  @spec get_resources(String.t()) :: list()
  def get_resources(_address) do
    []
  end

  # Helper functions

  defp build_resource(token, amount_wei, sender, nullifier_key_hex, forwarder_address) do
    nullifier_key = decode_hex(nullifier_key_hex)
    nonce = :crypto.strong_rand_bytes(32)
    rand_seed = :crypto.strong_rand_bytes(32)
    nk_commitment = :crypto.hash(:sha256, nullifier_key)

    %{
      logic_ref: Base.encode16(hash_logic_ref(forwarder_address), case: :lower),
      label_ref: Base.encode16(hash_label_ref(token), case: :lower),
      quantity: amount_wei,
      value_ref: Base.encode16(hash_value_ref(sender), case: :lower),
      is_ephemeral: false,
      nonce: Base.encode16(nonce, case: :lower),
      nk_commitment: Base.encode16(nk_commitment, case: :lower),
      rand_seed: Base.encode16(rand_seed, case: :lower)
    }
  end

  defp build_swap_resource(output_token, min_amount_wei, input_resource_map, nullifier_key_hex, output_forwarder) do
    nullifier_key = decode_hex(nullifier_key_hex)
    nonce = :crypto.strong_rand_bytes(32)
    rand_seed = :crypto.strong_rand_bytes(32)
    nk_commitment = :crypto.hash(:sha256, nullifier_key)

    %{
      logic_ref: Base.encode16(hash_logic_ref(output_forwarder), case: :lower),
      label_ref: Base.encode16(hash_label_ref(output_token), case: :lower),
      quantity: min_amount_wei,
      value_ref: input_resource_map["value_ref"] || input_resource_map[:value_ref] || Base.encode16(<<0::256>>, case: :lower),
      is_ephemeral: false,
      nonce: Base.encode16(nonce, case: :lower),
      nk_commitment: Base.encode16(nk_commitment, case: :lower),
      rand_seed: Base.encode16(rand_seed, case: :lower)
    }
  end

  defp decode_hex(hex) do
    hex
    |> String.replace_prefix("0x", "")
    |> Base.decode16!(case: :mixed)
  end

  defp get_forwarder_address(token) do
    case String.upcase(token) do
      "WETH" -> @contracts.weth_forwarder
      "USDC" -> @contracts.usdc_forwarder
      _ -> raise "Unknown token: #{token}"
    end
  end

  defp parse_amount(amount, decimals) when is_binary(amount) do
    {float_amount, _} = Float.parse(amount)
    round(float_amount * :math.pow(10, decimals))
  end

  defp parse_amount(amount, decimals) when is_number(amount) do
    round(amount * :math.pow(10, decimals))
  end

  defp hash_logic_ref(address), do: :crypto.hash(:sha256, address)
  defp hash_label_ref(token), do: :crypto.hash(:sha256, token)
  defp hash_value_ref(owner), do: :crypto.hash(:sha256, owner)

  defp generate_mock_proof do
    proof_bytes = :crypto.strong_rand_bytes(64)
    Base.encode16(proof_bytes, case: :lower)
  end

  defp encode_shield_call(sender, amount) do
    selector = "23b872dd"
    sender_padded = sender |> String.replace_prefix("0x", "") |> String.pad_leading(64, "0")
    recipient_padded = String.duplicate("0", 64)
    amount_hex = Integer.to_string(amount, 16) |> String.pad_leading(64, "0")
    "0x" <> selector <> sender_padded <> recipient_padded <> amount_hex
  end

  defp encode_unshield_call(recipient, amount) do
    selector = "a9059cbb"
    recipient_padded = recipient |> String.replace_prefix("0x", "") |> String.pad_leading(64, "0")
    amount_hex = Integer.to_string(amount, 16) |> String.pad_leading(64, "0")
    "0x" <> selector <> recipient_padded <> amount_hex
  end

  defp encode_swap_call(amount_in, amount_out_min, output_token) do
    selector = "414bf389"
    amount_in_hex = Integer.to_string(amount_in, 16) |> String.pad_leading(64, "0")
    amount_out_hex = Integer.to_string(amount_out_min, 16) |> String.pad_leading(64, "0")

    token_out = case String.upcase(output_token) do
      "WETH" -> @contracts.weth
      "USDC" -> @contracts.usdc
      _ -> @contracts.weth
    end

    token_padded = token_out |> String.replace_prefix("0x", "") |> String.pad_leading(64, "0")
    "0x" <> selector <> amount_in_hex <> amount_out_hex <> token_padded
  end

  defp decode_forwarder_from_logic_ref(logic_ref) when is_binary(logic_ref) do
    weth_hash = Base.encode16(hash_logic_ref(@contracts.weth_forwarder), case: :lower)
    usdc_hash = Base.encode16(hash_logic_ref(@contracts.usdc_forwarder), case: :lower)

    cond do
      logic_ref == weth_hash -> @contracts.weth_forwarder
      logic_ref == usdc_hash -> @contracts.usdc_forwarder
      true -> @contracts.weth_forwarder
    end
  end

  defp decode_forwarder_from_logic_ref(_), do: @contracts.weth_forwarder
end
