defmodule Backend.ResourceStore do
  @moduledoc """
  In-memory store for tracking shielded resources.
  In production, this would be replaced with a database or indexer.
  """

  use GenServer

  # Client API

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @doc """
  Store a new shielded resource.
  """
  def store_resource(commitment, resource_data) do
    GenServer.call(__MODULE__, {:store, commitment, resource_data})
  end

  @doc """
  Get a resource by its commitment.
  """
  def get_resource(commitment) do
    GenServer.call(__MODULE__, {:get, commitment})
  end

  @doc """
  Get all resources for a specific owner.
  """
  def get_resources_by_owner(owner) do
    GenServer.call(__MODULE__, {:get_by_owner, owner})
  end

  @doc """
  Mark a resource as spent (nullified).
  """
  def mark_spent(nullifier) do
    GenServer.call(__MODULE__, {:mark_spent, nullifier})
  end

  @doc """
  Check if a nullifier has been used.
  """
  def is_nullified?(nullifier) do
    GenServer.call(__MODULE__, {:is_nullified, nullifier})
  end

  @doc """
  Get all resources (for debugging).
  """
  def list_all do
    GenServer.call(__MODULE__, :list_all)
  end

  # Server callbacks

  @impl true
  def init(_opts) do
    # State: %{resources: %{commitment => resource_data}, nullifiers: MapSet, owners: %{owner => [commitment]}}
    {:ok, %{resources: %{}, nullifiers: MapSet.new(), owners: %{}}}
  end

  @impl true
  def handle_call({:store, commitment, resource_data}, _from, state) do
    owner = resource_data[:owner] || resource_data["owner"]

    # Update resources map
    resources = Map.put(state.resources, commitment, resource_data)

    # Update owners index
    owner_resources = Map.get(state.owners, owner, [])
    owners = Map.put(state.owners, owner, [commitment | owner_resources])

    {:reply, :ok, %{state | resources: resources, owners: owners}}
  end

  @impl true
  def handle_call({:get, commitment}, _from, state) do
    {:reply, Map.get(state.resources, commitment), state}
  end

  @impl true
  def handle_call({:get_by_owner, owner}, _from, state) do
    commitments = Map.get(state.owners, owner, [])

    resources =
      commitments
      |> Enum.map(fn c -> Map.get(state.resources, c) end)
      |> Enum.reject(&is_nil/1)
      |> Enum.reject(fn r -> MapSet.member?(state.nullifiers, r[:commitment]) end)

    {:reply, resources, state}
  end

  @impl true
  def handle_call({:mark_spent, nullifier}, _from, state) do
    nullifiers = MapSet.put(state.nullifiers, nullifier)
    {:reply, :ok, %{state | nullifiers: nullifiers}}
  end

  @impl true
  def handle_call({:is_nullified, nullifier}, _from, state) do
    {:reply, MapSet.member?(state.nullifiers, nullifier), state}
  end

  @impl true
  def handle_call(:list_all, _from, state) do
    {:reply, state.resources, state}
  end
end
