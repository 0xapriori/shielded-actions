// API client for the Shielded Actions backend

// Backend URL - defaults to localhost for development
export const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:4000'

export interface ShieldRequest {
  token: string
  amount: string
  sender: string
  nullifier_key: string
}

export interface ShieldResponse {
  transaction: string
  resource_commitment: string
  resource: Resource
  forwarder_call: {
    to: string
    data: string
  }
}

export interface SwapRequest {
  input_resource: Resource
  output_token: string
  nullifier_key: string
  min_amount_out: string
}

export interface SwapResponse {
  transaction: string
  nullifier: string
  new_resource_commitment: string
  new_resource: Resource
  uniswap_call: {
    to: string
    data: string
  }
}

export interface UnshieldRequest {
  resource: Resource
  recipient: string
  nullifier_key: string
}

export interface UnshieldResponse {
  transaction: string
  nullifier: string
  forwarder_call: {
    to: string
    data: string
  }
}

export interface Resource {
  logic_ref: string
  label_ref: string
  quantity: number
  value_ref: string
  is_ephemeral: boolean
  nonce: string
  nk_commitment: string
  rand_seed: string
}

export interface Keypair {
  private_key: string
  public_key: string
}

export interface ApiError {
  error: string
}

// API Info
export async function getApiInfo() {
  const response = await fetch(`${API_BASE_URL}/api/info`)
  if (!response.ok) {
    throw new Error('Failed to fetch API info')
  }
  return response.json()
}

// Health check
export async function healthCheck(): Promise<boolean> {
  try {
    const response = await fetch(`${API_BASE_URL}/health`)
    return response.ok
  } catch {
    return false
  }
}

// Generate a new keypair
export async function generateKeypair(): Promise<Keypair> {
  const response = await fetch(`${API_BASE_URL}/api/generate-keypair`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
  })

  if (!response.ok) {
    const error: ApiError = await response.json()
    throw new Error(error.error)
  }

  return response.json()
}

// Create a shield transaction
export async function createShieldTransaction(request: ShieldRequest): Promise<ShieldResponse> {
  const response = await fetch(`${API_BASE_URL}/api/shield`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })

  if (!response.ok) {
    const error: ApiError = await response.json()
    throw new Error(error.error)
  }

  return response.json()
}

// Create a swap transaction
export async function createSwapTransaction(request: SwapRequest): Promise<SwapResponse> {
  const response = await fetch(`${API_BASE_URL}/api/swap`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })

  if (!response.ok) {
    const error: ApiError = await response.json()
    throw new Error(error.error)
  }

  return response.json()
}

// Create an unshield transaction
export async function createUnshieldTransaction(request: UnshieldRequest): Promise<UnshieldResponse> {
  const response = await fetch(`${API_BASE_URL}/api/unshield`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })

  if (!response.ok) {
    const error: ApiError = await response.json()
    throw new Error(error.error)
  }

  return response.json()
}

// Get resources for an address
export async function getResources(address: string): Promise<{ address: string; resources: Resource[] }> {
  const response = await fetch(`${API_BASE_URL}/api/resources/${address}`)

  if (!response.ok) {
    const error: ApiError = await response.json()
    throw new Error(error.error)
  }

  return response.json()
}

// Local storage helpers for keypair management
const KEYPAIR_STORAGE_KEY = 'shielded-actions-keypair'

export function getStoredKeypair(): Keypair | null {
  const stored = localStorage.getItem(KEYPAIR_STORAGE_KEY)
  if (stored) {
    try {
      return JSON.parse(stored)
    } catch {
      return null
    }
  }
  return null
}

export function storeKeypair(keypair: Keypair): void {
  localStorage.setItem(KEYPAIR_STORAGE_KEY, JSON.stringify(keypair))
}

export function clearKeypair(): void {
  localStorage.removeItem(KEYPAIR_STORAGE_KEY)
}

// Resource storage helpers
const RESOURCES_STORAGE_KEY = 'shielded-actions-resources'

export function getStoredResources(): Resource[] {
  const stored = localStorage.getItem(RESOURCES_STORAGE_KEY)
  if (stored) {
    try {
      return JSON.parse(stored)
    } catch {
      return []
    }
  }
  return []
}

export function storeResource(resource: Resource): void {
  const resources = getStoredResources()
  resources.push(resource)
  localStorage.setItem(RESOURCES_STORAGE_KEY, JSON.stringify(resources))
}

export function removeResource(commitment: string): void {
  const resources = getStoredResources().filter(
    r => r.nonce !== commitment // Using nonce as a proxy for identification
  )
  localStorage.setItem(RESOURCES_STORAGE_KEY, JSON.stringify(resources))
}

export function clearResources(): void {
  localStorage.removeItem(RESOURCES_STORAGE_KEY)
}
