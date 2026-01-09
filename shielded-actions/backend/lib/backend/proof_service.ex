defmodule Backend.ProofService do
  @moduledoc """
  Service for generating Anoma Resource Machine proofs.
  Handles shield, swap, and unshield operations using the Anoma SDK.
  """

  alias AnomaSDK.Arm
  alias AnomaSDK.Arm.Action
  alias AnomaSDK.Arm.ComplianceWitness
  alias AnomaSDK.Arm.DeltaWitness
  alias AnomaSDK.Arm.NullifierKey
  alias AnomaSDK.Arm.Resource
  alias AnomaSDK.Arm.Transaction

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

  @doc """
  Generate a new nullifier key pair for a user.
  Returns both the private key and the commitment.
  """
  @spec generate_keypair() :: {:ok, map()} | {:error, String.t()}
  def generate_keypair do
    try do
      keypair = Arm.random_key_pair()

      {:ok,
       %{
         private_key: Base.encode16(keypair.secret_key, case: :lower),
         public_key: Base.encode16(keypair.public_key, case: :lower)
       }}
    rescue
      e ->
        {:error, "Failed to generate keypair: #{inspect(e)}"}
    end
  end

  @doc """
  Create a shield transaction that converts ERC20 tokens to shielded resources.
  """
  @spec create_shield_transaction(String.t(), String.t(), String.t(), String.t()) ::
          {:ok, map()} | {:error, String.t()}
  def create_shield_transaction(token, amount, sender, nullifier_key_hex) do
    try do
      # Parse the nullifier key
      nullifier_key = decode_hex(nullifier_key_hex)

      # Get the forwarder address for this token
      forwarder_address = get_forwarder_address(token)

      # Parse the amount to the correct decimal precision
      decimals = Map.get(@token_decimals, String.upcase(token), 18)
      amount_wei = parse_amount(amount, decimals)

      # Create the resource that will be created (shielded token)
      nk_commitment = NullifierKey.commit(nullifier_key)

      created_resource = %Resource{
        logic_ref: hash_logic_ref(forwarder_address),
        label_ref: hash_label_ref(token),
        quantity: amount_wei,
        value_ref: hash_value_ref(sender),
        is_ephemeral: false,
        nonce: :crypto.strong_rand_bytes(32),
        nk_commitment: nk_commitment,
        rand_seed: :crypto.strong_rand_bytes(32)
      }

      # Create a "zero" consumed resource for shielding (minting new resource)
      consumed_resource = create_zero_resource(nk_commitment)

      # Build the compliance witness
      compliance_witness =
        ComplianceWitness.with_fixed_rcv(consumed_resource, nullifier_key, created_resource)

      # Generate compliance proof
      compliance_unit = Arm.prove_compliance_witness(compliance_witness)

      # Build the action
      action = %Action{
        compliance_units: [compliance_unit],
        logic_verifier_inputs: []
      }

      # Build the delta witness (for signing)
      delta_witness = %DeltaWitness{
        signing_key: nullifier_key
      }

      # Build the transaction
      transaction = %Transaction{
        actions: [action],
        delta_proof: {:witness, delta_witness}
      }

      # Generate the delta proof
      transaction_with_proof = Transaction.generate_delta_proof(transaction)

      # Compute the resource commitment for tracking
      resource_commitment = Resource.commitment(created_resource)

      {:ok,
       %{
         transaction: encode_transaction(transaction_with_proof),
         resource_commitment: Base.encode16(resource_commitment, case: :lower),
         resource: encode_resource(created_resource),
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
    try do
      # Parse the nullifier key
      nullifier_key = decode_hex(nullifier_key_hex)

      # Decode the input resource
      input_resource = Resource.from_map(atomize_keys(input_resource_map))

      # Parse min amount out
      output_decimals = Map.get(@token_decimals, String.upcase(output_token), 18)
      min_amount_wei = parse_amount(min_amount_out, output_decimals)

      # Create the output resource (new shielded token after swap)
      nk_commitment = NullifierKey.commit(nullifier_key)

      output_forwarder = get_forwarder_address(output_token)

      created_resource = %Resource{
        logic_ref: hash_logic_ref(output_forwarder),
        label_ref: hash_label_ref(output_token),
        quantity: min_amount_wei,
        value_ref: input_resource.value_ref,
        is_ephemeral: false,
        nonce: :crypto.strong_rand_bytes(32),
        nk_commitment: nk_commitment,
        rand_seed: :crypto.strong_rand_bytes(32)
      }

      # Build the compliance witness
      compliance_witness =
        ComplianceWitness.with_fixed_rcv(input_resource, nullifier_key, created_resource)

      # Generate compliance proof
      compliance_unit = Arm.prove_compliance_witness(compliance_witness)

      # Build the action
      action = %Action{
        compliance_units: [compliance_unit],
        logic_verifier_inputs: []
      }

      # Build the delta witness
      delta_witness = %DeltaWitness{
        signing_key: nullifier_key
      }

      # Build the transaction
      transaction = %Transaction{
        actions: [action],
        delta_proof: {:witness, delta_witness}
      }

      # Generate the delta proof
      transaction_with_proof = Transaction.generate_delta_proof(transaction)

      # Compute nullifier of consumed resource
      nullifier = Resource.nullifier(input_resource, nullifier_key)

      # Compute the new resource commitment
      resource_commitment = Resource.commitment(created_resource)

      {:ok,
       %{
         transaction: encode_transaction(transaction_with_proof),
         nullifier: Base.encode16(nullifier, case: :lower),
         new_resource_commitment: Base.encode16(resource_commitment, case: :lower),
         new_resource: encode_resource(created_resource),
         uniswap_call: %{
           to: @contracts.uniswap_forwarder,
           data: encode_swap_call(input_resource.quantity, min_amount_wei, output_token)
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
    try do
      # Parse the nullifier key
      nullifier_key = decode_hex(nullifier_key_hex)

      # Decode the resource
      resource = Resource.from_map(atomize_keys(resource_map))

      # Verify the nullifier key matches the resource
      computed_commitment = NullifierKey.commit(nullifier_key)

      if resource.nk_commitment != computed_commitment do
        {:error, "Nullifier key does not match resource commitment"}
      else
        # Create a "zero" output resource (burning the shielded resource)
        nk_commitment = NullifierKey.commit(nullifier_key)
        zero_resource = create_zero_resource(nk_commitment)

        # Build the compliance witness
        compliance_witness =
          ComplianceWitness.with_fixed_rcv(resource, nullifier_key, zero_resource)

        # Generate compliance proof
        compliance_unit = Arm.prove_compliance_witness(compliance_witness)

        # Build the action
        action = %Action{
          compliance_units: [compliance_unit],
          logic_verifier_inputs: []
        }

        # Build the delta witness
        delta_witness = %DeltaWitness{
          signing_key: nullifier_key
        }

        # Build the transaction
        transaction = %Transaction{
          actions: [action],
          delta_proof: {:witness, delta_witness}
        }

        # Generate the delta proof
        transaction_with_proof = Transaction.generate_delta_proof(transaction)

        # Compute nullifier
        nullifier = Resource.nullifier(resource, nullifier_key)

        # Determine the forwarder from the resource logic_ref
        forwarder_address = decode_forwarder_from_logic_ref(resource.logic_ref)

        {:ok,
         %{
           transaction: encode_transaction(transaction_with_proof),
           nullifier: Base.encode16(nullifier, case: :lower),
           forwarder_call: %{
             to: forwarder_address,
             data: encode_unshield_call(recipient, resource.quantity)
           }
         }}
      end
    rescue
      e ->
        {:error, "Failed to create unshield transaction: #{inspect(e)}"}
    end
  end

  @doc """
  Get shielded resources for an address (placeholder for demo).
  In production, this would query an indexer or database.
  """
  @spec get_resources(String.t()) :: list()
  def get_resources(_address) do
    # This is a placeholder - in production, you would:
    # 1. Query an indexer for resource commitments
    # 2. Filter by those that belong to the user (using their viewing key)
    []
  end

  # Private helper functions

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

  defp hash_logic_ref(address) do
    :crypto.hash(:sha256, address)
  end

  defp hash_label_ref(token) do
    :crypto.hash(:sha256, token)
  end

  defp hash_value_ref(owner) do
    :crypto.hash(:sha256, owner)
  end

  defp create_zero_resource(nk_commitment) do
    %Resource{
      logic_ref: <<0::256>>,
      label_ref: <<0::256>>,
      quantity: 0,
      value_ref: <<0::256>>,
      is_ephemeral: true,
      nonce: <<0::256>>,
      nk_commitment: nk_commitment,
      rand_seed: <<0::256>>
    }
  end

  defp encode_transaction(transaction) do
    Jason.encode!(transaction)
  end

  defp encode_resource(resource) do
    %{
      logic_ref: Base.encode16(resource.logic_ref, case: :lower),
      label_ref: Base.encode16(resource.label_ref, case: :lower),
      quantity: resource.quantity,
      value_ref: Base.encode16(resource.value_ref, case: :lower),
      is_ephemeral: resource.is_ephemeral,
      nonce: Base.encode16(resource.nonce, case: :lower),
      nk_commitment: Base.encode16(resource.nk_commitment, case: :lower),
      rand_seed: Base.encode16(resource.rand_seed, case: :lower)
    }
  end

  # Encode the ERC20 transferFrom call for shielding
  defp encode_shield_call(sender, amount) do
    # Function signature: transferFrom(address,address,uint256)
    # selector: 0x23b872dd
    selector = "23b872dd"

    sender_padded =
      sender
      |> String.replace_prefix("0x", "")
      |> String.pad_leading(64, "0")

    # Recipient is the forwarder itself (represented as zeros, actual value set by contract)
    recipient_padded = String.duplicate("0", 64)
    amount_hex = Integer.to_string(amount, 16) |> String.pad_leading(64, "0")

    "0x" <> selector <> sender_padded <> recipient_padded <> amount_hex
  end

  # Encode the ERC20 transfer call for unshielding
  defp encode_unshield_call(recipient, amount) do
    # Function signature: transfer(address,uint256)
    # selector: 0xa9059cbb
    selector = "a9059cbb"

    recipient_padded =
      recipient
      |> String.replace_prefix("0x", "")
      |> String.pad_leading(64, "0")

    amount_hex = Integer.to_string(amount, 16) |> String.pad_leading(64, "0")

    "0x" <> selector <> recipient_padded <> amount_hex
  end

  # Encode Uniswap V3 exactInputSingle call
  defp encode_swap_call(amount_in, amount_out_min, output_token) do
    # Function signature: exactInputSingle(ExactInputSingleParams)
    # selector: 0x414bf389
    selector = "414bf389"

    # This is simplified - actual encoding would need the full struct
    amount_in_hex = Integer.to_string(amount_in, 16) |> String.pad_leading(64, "0")
    amount_out_hex = Integer.to_string(amount_out_min, 16) |> String.pad_leading(64, "0")

    token_out =
      case String.upcase(output_token) do
        "WETH" -> @contracts.weth
        "USDC" -> @contracts.usdc
        _ -> @contracts.weth
      end

    token_padded =
      token_out
      |> String.replace_prefix("0x", "")
      |> String.pad_leading(64, "0")

    "0x" <> selector <> amount_in_hex <> amount_out_hex <> token_padded
  end

  defp decode_forwarder_from_logic_ref(logic_ref) do
    # Try to match against known forwarder hashes
    weth_hash = hash_logic_ref(@contracts.weth_forwarder)
    usdc_hash = hash_logic_ref(@contracts.usdc_forwarder)

    cond do
      logic_ref == weth_hash -> @contracts.weth_forwarder
      logic_ref == usdc_hash -> @contracts.usdc_forwarder
      true -> @contracts.weth_forwarder
    end
  end

  defp atomize_keys(map) when is_map(map) do
    Map.new(map, fn
      {k, v} when is_binary(k) -> {String.to_atom(k), atomize_keys(v)}
      {k, v} -> {k, atomize_keys(v)}
    end)
  end

  defp atomize_keys(list) when is_list(list), do: Enum.map(list, &atomize_keys/1)
  defp atomize_keys(value), do: value
end
