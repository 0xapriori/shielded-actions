// API client for the Shielded Actions backend
// Uses async polling for proof generation (proofs take ~7 minutes)

// Prover URL - talks directly to prover for async job support
export const PROVER_URL = import.meta.env.VITE_PROVER_URL || 'http://localhost:3002'

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
  calldata?: string
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
  calldata?: string
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
  calldata?: string
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

// Job status response
interface JobResponse {
  job_id: string
  status: 'pending' | 'generating' | 'completed' | 'failed'
  calldata?: string
  proof_id?: string
  error?: string
  result?: {
    transaction: string
    resource_commitment: string
    calldata: string
    forwarder_call: {
      data: string
    }
  }
}

// Polling interval in ms
const POLL_INTERVAL = 2000

// Health check
export async function healthCheck(): Promise<boolean> {
  try {
    const response = await fetch(`${PROVER_URL}/health`)
    return response.ok
  } catch {
    return false
  }
}

// Generate a new keypair
export async function generateKeypair(): Promise<Keypair> {
  const response = await fetch(`${PROVER_URL}/api/generate-keypair`, {
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

// Poll for job completion
async function pollForCompletion(
  jobId: string,
  onStatusUpdate?: (status: string) => void
): Promise<JobResponse> {
  const maxAttempts = 300 // 10 minutes at 2 second intervals
  let attempts = 0

  while (attempts < maxAttempts) {
    const response = await fetch(`${PROVER_URL}/api/job/${jobId}`)

    if (!response.ok) {
      throw new Error(`Failed to check job status: ${response.statusText}`)
    }

    const job: JobResponse = await response.json()

    if (onStatusUpdate) {
      onStatusUpdate(job.status)
    }

    if (job.status === 'completed') {
      return job
    }

    if (job.status === 'failed') {
      throw new Error(job.error || 'Proof generation failed')
    }

    // Wait before next poll
    await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL))
    attempts++
  }

  throw new Error('Proof generation timed out after 10 minutes')
}

// Create a shield transaction (async with polling)
export async function createShieldTransaction(
  request: ShieldRequest,
  onStatusUpdate?: (status: string) => void
): Promise<ShieldResponse> {
  // Start the job
  const startResponse = await fetch(`${PROVER_URL}/api/shield`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })

  if (!startResponse.ok) {
    const error: ApiError = await startResponse.json()
    throw new Error(error.error)
  }

  const { job_id } = await startResponse.json()

  if (onStatusUpdate) {
    onStatusUpdate('pending')
  }

  // Poll for completion
  const result = await pollForCompletion(job_id, onStatusUpdate)

  // Build response from job result
  return {
    transaction: result.proof_id || job_id,
    resource_commitment: result.result?.resource_commitment || `0x${job_id}`,
    resource: {
      logic_ref: '',
      label_ref: '',
      quantity: 0,
      value_ref: '',
      is_ephemeral: true,
      nonce: job_id,
      nk_commitment: '',
      rand_seed: ''
    },
    forwarder_call: {
      to: '',
      data: result.calldata || ''
    },
    calldata: result.calldata
  }
}

// Create a swap transaction (async with polling)
export async function createSwapTransaction(
  request: SwapRequest,
  onStatusUpdate?: (status: string) => void
): Promise<SwapResponse> {
  const startResponse = await fetch(`${PROVER_URL}/api/swap`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })

  if (!startResponse.ok) {
    const error: ApiError = await startResponse.json()
    throw new Error(error.error)
  }

  const { job_id } = await startResponse.json()

  if (onStatusUpdate) {
    onStatusUpdate('pending')
  }

  const result = await pollForCompletion(job_id, onStatusUpdate)

  return {
    transaction: result.proof_id || job_id,
    nullifier: `0x${job_id}`,
    new_resource_commitment: result.result?.resource_commitment || `0x${job_id}`,
    new_resource: {
      logic_ref: '',
      label_ref: '',
      quantity: 0,
      value_ref: '',
      is_ephemeral: true,
      nonce: job_id,
      nk_commitment: '',
      rand_seed: ''
    },
    uniswap_call: {
      to: '0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA',
      data: result.calldata || ''
    },
    calldata: result.calldata
  }
}

// Create an unshield transaction (async with polling)
export async function createUnshieldTransaction(
  request: UnshieldRequest,
  onStatusUpdate?: (status: string) => void
): Promise<UnshieldResponse> {
  const startResponse = await fetch(`${PROVER_URL}/api/unshield`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })

  if (!startResponse.ok) {
    const error: ApiError = await startResponse.json()
    throw new Error(error.error)
  }

  const { job_id } = await startResponse.json()

  if (onStatusUpdate) {
    onStatusUpdate('pending')
  }

  const result = await pollForCompletion(job_id, onStatusUpdate)

  return {
    transaction: result.proof_id || job_id,
    nullifier: `0x${job_id}`,
    forwarder_call: {
      to: '',
      data: result.calldata || ''
    },
    calldata: result.calldata
  }
}

// Get resources for an address
export async function getResources(address: string): Promise<{ address: string; resources: Resource[] }> {
  // For now, return empty - resources are stored locally
  return { address, resources: [] }
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
    r => r.nonce !== commitment
  )
  localStorage.setItem(RESOURCES_STORAGE_KEY, JSON.stringify(resources))
}

export function clearResources(): void {
  localStorage.removeItem(RESOURCES_STORAGE_KEY)
}
