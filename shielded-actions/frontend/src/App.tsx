import { useState, useEffect } from 'react'
import { ConnectButton } from '@rainbow-me/rainbowkit'
import { useAccount, useReadContract, useWriteContract, useWaitForTransactionReceipt, useSendTransaction } from 'wagmi'
import { formatUnits, parseUnits, type Hex } from 'viem'
import { CONTRACTS, ERC20_ABI, ERC20_FORWARDER_ABI } from './contracts'
import * as api from './api'

type Token = 'WETH' | 'USDC'

// Transaction execution states
type TxState = 'idle' | 'pending' | 'generating' | 'generating_proof' | 'awaiting_signature' | 'confirming' | 'success' | 'error'

const TOKENS: Record<Token, { address: `0x${string}`; forwarder: `0x${string}`; decimals: number; symbol: string }> = {
  WETH: { address: CONTRACTS.WETH, forwarder: CONTRACTS.WETH_FORWARDER, decimals: 18, symbol: 'WETH' },
  USDC: { address: CONTRACTS.USDC, forwarder: CONTRACTS.USDC_FORWARDER, decimals: 6, symbol: 'USDC' },
}

function App() {
  const { address, isConnected } = useAccount()
  const [selectedToken, setSelectedToken] = useState<Token>('WETH')
  const [amount, setAmount] = useState('')
  const [activeTab, setActiveTab] = useState<'shield' | 'swap' | 'unshield'>('shield')
  const [keypair, setKeypair] = useState<api.Keypair | null>(null)
  const [shieldedResources, setShieldedResources] = useState<api.Resource[]>([])
  const [selectedResource, setSelectedResource] = useState<api.Resource | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [backendStatus, setBackendStatus] = useState<'checking' | 'online' | 'offline'>('checking')
  const [txState, setTxState] = useState<TxState>('idle')
  const [proofTxHash, setProofTxHash] = useState<Hex | undefined>(undefined)

  const token = TOKENS[selectedToken]

  // Check backend status on load
  useEffect(() => {
    async function checkBackend() {
      const isOnline = await api.healthCheck()
      setBackendStatus(isOnline ? 'online' : 'offline')
    }
    checkBackend()
  }, [])

  // Load keypair and resources from storage on mount
  useEffect(() => {
    const stored = api.getStoredKeypair()
    if (stored) {
      setKeypair(stored)
    }
    setShieldedResources(api.getStoredResources())
  }, [])

  // Read token balance
  const { data: balance } = useReadContract({
    address: token.address,
    abi: ERC20_ABI,
    functionName: 'balanceOf',
    args: address ? [address] : undefined,
    query: { enabled: !!address }
  })

  // Read allowance - refetch after approval
  const { data: allowance, refetch: refetchAllowance } = useReadContract({
    address: token.address,
    abi: ERC20_ABI,
    functionName: 'allowance',
    args: address ? [address, token.forwarder] : undefined,
    query: { enabled: !!address }
  })

  // Read forwarder balance (shielded pool)
  const { data: poolBalance } = useReadContract({
    address: token.forwarder,
    abi: ERC20_FORWARDER_ABI,
    functionName: 'getBalance',
  })

  // Write contracts (for approve)
  const { writeContract, data: txHash, isPending } = useWriteContract()
  const { isLoading: isConfirming, isSuccess } = useWaitForTransactionReceipt({ hash: txHash })

  // Send raw transaction (for proof execution)
  const { sendTransaction, data: proofSentHash } = useSendTransaction()
  const { isSuccess: isProofSuccess } = useWaitForTransactionReceipt({ hash: proofTxHash })

  // Update proofTxHash when transaction is sent
  useEffect(() => {
    if (proofSentHash) {
      setProofTxHash(proofSentHash)
      setTxState('confirming')
    }
  }, [proofSentHash])

  // Update state when proof tx confirms
  useEffect(() => {
    if (isProofSuccess && proofTxHash) {
      setTxState('success')
    }
  }, [isProofSuccess, proofTxHash])

  // Refetch allowance after approval transaction succeeds
  useEffect(() => {
    if (isSuccess && txHash) {
      refetchAllowance()
    }
  }, [isSuccess, txHash, refetchAllowance])

  const handleApprove = () => {
    const amountWei = parseUnits(amount || '0', token.decimals)
    writeContract({
      address: token.address,
      abi: ERC20_ABI,
      functionName: 'approve',
      args: [token.forwarder, amountWei]
    })
  }

  const needsApproval = () => {
    if (!amount || !allowance) return true
    const amountWei = parseUnits(amount, token.decimals)
    return (allowance as bigint) < amountWei
  }

  // Generate a new keypair
  const handleGenerateKeypair = async () => {
    setIsLoading(true)
    setError(null)
    try {
      const newKeypair = await api.generateKeypair()
      setKeypair(newKeypair)
      api.storeKeypair(newKeypair)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to generate keypair')
    } finally {
      setIsLoading(false)
    }
  }

  // Execute proof on-chain via Protocol Adapter
  const executeProofOnChain = async (calldata: string) => {
    setTxState('awaiting_signature')
    try {
      // Send transaction to Protocol Adapter with the proof calldata
      sendTransaction({
        to: CONTRACTS.PROTOCOL_ADAPTER as Hex,
        data: calldata as Hex,
        gas: BigInt(1500000), // Set higher gas limit for ZK proof verification
      })
    } catch (e) {
      setTxState('error')
      throw e
    }
  }

  // Shield tokens
  const handleShield = async () => {
    if (!keypair || !address || !amount) return

    setIsLoading(true)
    setError(null)
    setTxState('pending')
    setProofTxHash(undefined)

    try {
      const result = await api.createShieldTransaction(
        {
          token: selectedToken,
          amount,
          sender: address,
          nullifier_key: keypair.private_key,
        },
        // Status update callback - updates UI as proof generates
        (status) => {
          if (status === 'generating') {
            setTxState('generating')
          } else if (status === 'pending') {
            setTxState('pending')
          }
        }
      )

      // Store the resource locally
      api.storeResource(result.resource)
      setShieldedResources(api.getStoredResources())

      // If we have calldata, execute on-chain
      if (result.calldata) {
        await executeProofOnChain(result.calldata)
      } else {
        setTxState('idle')
        alert(`Shield transaction created!\n\nResource commitment: ${result.resource_commitment}`)
      }
    } catch (e) {
      setTxState('error')
      setError(e instanceof Error ? e.message : 'Failed to create shield transaction')
    } finally {
      setIsLoading(false)
    }
  }

  // Swap shielded tokens
  const handleSwap = async () => {
    if (!keypair || !selectedResource || !amount) return

    setIsLoading(true)
    setError(null)
    setTxState('pending')
    setProofTxHash(undefined)

    try {
      const outputToken = selectedToken === 'WETH' ? 'USDC' : 'WETH'
      const result = await api.createSwapTransaction(
        {
          input_resource: selectedResource,
          output_token: outputToken,
          nullifier_key: keypair.private_key,
          min_amount_out: amount,
        },
        (status) => {
          if (status === 'generating') setTxState('generating')
          else if (status === 'pending') setTxState('pending')
        }
      )

      // Update local resources
      api.removeResource(selectedResource.nonce)
      api.storeResource(result.new_resource)
      setShieldedResources(api.getStoredResources())
      setSelectedResource(null)

      // If we have calldata, execute on-chain
      if (result.calldata) {
        await executeProofOnChain(result.calldata)
      } else {
        setTxState('idle')
        alert(`Swap transaction created!\n\nNullifier: ${result.nullifier}\nNew commitment: ${result.new_resource_commitment}`)
      }
    } catch (e) {
      setTxState('error')
      setError(e instanceof Error ? e.message : 'Failed to create swap transaction')
    } finally {
      setIsLoading(false)
    }
  }

  // Unshield tokens
  const handleUnshield = async () => {
    if (!keypair || !selectedResource || !address) return

    setIsLoading(true)
    setError(null)
    setTxState('pending')
    setProofTxHash(undefined)

    try {
      const result = await api.createUnshieldTransaction(
        {
          resource: selectedResource,
          recipient: address,
          nullifier_key: keypair.private_key,
        },
        (status) => {
          if (status === 'generating') setTxState('generating')
          else if (status === 'pending') setTxState('pending')
        }
      )

      // Remove the resource from local storage
      api.removeResource(selectedResource.nonce)
      setShieldedResources(api.getStoredResources())
      setSelectedResource(null)

      // If we have calldata, execute on-chain
      if (result.calldata) {
        await executeProofOnChain(result.calldata)
      } else {
        setTxState('idle')
        alert(`Unshield transaction created!\n\nNullifier: ${result.nullifier}`)
      }
    } catch (e) {
      setTxState('error')
      setError(e instanceof Error ? e.message : 'Failed to create unshield transaction')
    } finally {
      setIsLoading(false)
    }
  }

  // Reset transaction state
  const resetTxState = () => {
    setTxState('idle')
    setProofTxHash(undefined)
    setError(null)
  }

  return (
    <div style={styles.container}>
      <header style={styles.header}>
        <h1 style={styles.title}>Shielded Actions</h1>
        <p style={styles.subtitle}>Privacy-Preserving Swaps via Anoma Protocol Adapter</p>
        <div style={styles.connectWrapper}>
          <ConnectButton />
        </div>
        <div style={styles.statusBar}>
          <span style={{
            ...styles.statusDot,
            backgroundColor: backendStatus === 'online' ? '#22c55e' : backendStatus === 'offline' ? '#ef4444' : '#f59e0b'
          }} />
          Backend: {backendStatus === 'checking' ? 'Checking...' : backendStatus}
        </div>
      </header>

      {isConnected ? (
        <main style={styles.main}>
          {/* Keypair Management */}
          <div style={styles.keypairSection}>
            <h3 style={styles.sectionTitle}>Nullifier Key</h3>
            {keypair ? (
              <div style={styles.keypairInfo}>
                <div style={styles.keyDisplay}>
                  <span style={styles.keyLabel}>Private Key:</span>
                  <code style={styles.keyValue}>{keypair.private_key.slice(0, 16)}...{keypair.private_key.slice(-8)}</code>
                </div>
                <button onClick={() => { api.clearKeypair(); setKeypair(null) }} style={styles.smallButton}>
                  Clear Key
                </button>
              </div>
            ) : (
              <button
                onClick={handleGenerateKeypair}
                disabled={isLoading || backendStatus !== 'online'}
                style={styles.generateButton}
              >
                {isLoading ? 'Generating...' : 'Generate Nullifier Key'}
              </button>
            )}
          </div>

          {/* Tab Navigation */}
          <div style={styles.tabs}>
            {(['shield', 'swap', 'unshield'] as const).map((tab) => (
              <button
                key={tab}
                onClick={() => setActiveTab(tab)}
                style={{
                  ...styles.tab,
                  ...(activeTab === tab ? styles.tabActive : {})
                }}
              >
                {tab === 'shield' && '1. Shield'}
                {tab === 'swap' && '2. Swap'}
                {tab === 'unshield' && '3. Unshield'}
              </button>
            ))}
          </div>

          {/* Error Display */}
          {error && (
            <div style={styles.errorBox}>
              {error}
              <button onClick={() => setError(null)} style={styles.dismissButton}>×</button>
            </div>
          )}

          {/* Main Card */}
          <div style={styles.card}>
            {/* Token Selector */}
            <div style={styles.inputGroup}>
              <label style={styles.label}>Token</label>
              <select
                value={selectedToken}
                onChange={(e) => setSelectedToken(e.target.value as Token)}
                style={styles.select}
              >
                <option value="WETH">WETH</option>
                <option value="USDC">USDC</option>
              </select>
            </div>

            {/* Amount Input */}
            <div style={styles.inputGroup}>
              <label style={styles.label}>Amount</label>
              <input
                type="number"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                placeholder="0.0"
                style={styles.input}
              />
              <div style={styles.balance}>
                Balance: {balance ? formatUnits(balance as bigint, token.decimals) : '0'} {token.symbol}
              </div>
            </div>

            {/* Shield Tab Content */}
            {activeTab === 'shield' && (
              <div style={styles.tabContent}>
                <p style={styles.description}>
                  Shield your tokens to convert them into private Anoma resources.
                  Your tokens will be held in escrow by the forwarder contract.
                </p>

                <div style={styles.infoBox}>
                  <strong>Pool Balance:</strong> {poolBalance ? formatUnits(poolBalance as bigint, token.decimals) : '0'} {token.symbol}
                </div>

                {!keypair ? (
                  <p style={styles.warningText}>Please generate a nullifier key first.</p>
                ) : needsApproval() ? (
                  <button
                    onClick={handleApprove}
                    disabled={isPending || isConfirming || !amount}
                    style={styles.button}
                  >
                    {isPending ? 'Confirming...' : isConfirming ? 'Waiting...' : `Approve ${token.symbol}`}
                  </button>
                ) : (
                  <button
                    onClick={handleShield}
                    disabled={!amount || isLoading || backendStatus !== 'online' || txState !== 'idle'}
                    style={styles.button}
                  >
                    {txState === 'pending' ? 'Starting Proof...' :
                     txState === 'generating' ? 'Generating ZK Proof (~7 min)...' :
                     txState === 'generating_proof' ? 'Generating Proof...' :
                     txState === 'awaiting_signature' ? 'Confirm in Wallet...' :
                     txState === 'confirming' ? 'Confirming...' :
                     isLoading ? 'Creating Proof...' :
                     `Shield ${amount || '0'} ${token.symbol}`}
                  </button>
                )}
              </div>
            )}

            {/* Swap Tab Content */}
            {activeTab === 'swap' && (
              <div style={styles.tabContent}>
                <p style={styles.description}>
                  Swap your shielded tokens privately via Uniswap V3.
                  The swap details are hidden using zero-knowledge proofs.
                </p>

                {/* Resource Selector */}
                <div style={styles.inputGroup}>
                  <label style={styles.label}>Select Shielded Resource</label>
                  <select
                    style={styles.select}
                    value={selectedResource?.nonce || ''}
                    onChange={(e) => {
                      const resource = shieldedResources.find(r => r.nonce === e.target.value)
                      setSelectedResource(resource || null)
                    }}
                  >
                    <option value="">Select a resource...</option>
                    {shieldedResources.map((r, i) => (
                      <option key={i} value={r.nonce}>
                        Resource {i + 1} - Qty: {r.quantity}
                      </option>
                    ))}
                  </select>
                </div>

                <div style={styles.inputGroup}>
                  <label style={styles.label}>Swap To</label>
                  <select style={styles.select}>
                    <option value="USDC">{selectedToken === 'WETH' ? 'USDC' : 'WETH'}</option>
                  </select>
                </div>

                <div style={styles.infoBox}>
                  <strong>Route:</strong> {selectedToken} → Uniswap V3 → {selectedToken === 'WETH' ? 'USDC' : 'WETH'}
                </div>

                <button
                  onClick={handleSwap}
                  disabled={!selectedResource || !keypair || isLoading || backendStatus !== 'online' || txState !== 'idle'}
                  style={styles.button}
                >
                  {txState === 'pending' ? 'Starting Proof...' :
                   txState === 'generating' ? 'Generating ZK Proof (~7 min)...' :
                   txState === 'generating_proof' ? 'Generating Proof...' :
                   txState === 'awaiting_signature' ? 'Confirm in Wallet...' :
                   txState === 'confirming' ? 'Confirming...' :
                   isLoading ? 'Creating Proof...' :
                   `Swap Shielded ${amount || '0'} ${token.symbol}`}
                </button>
              </div>
            )}

            {/* Unshield Tab Content */}
            {activeTab === 'unshield' && (
              <div style={styles.tabContent}>
                <p style={styles.description}>
                  Unshield your tokens to withdraw them back to your wallet.
                  This reveals the final balance but not the transaction history.
                </p>

                {/* Resource Selector */}
                <div style={styles.inputGroup}>
                  <label style={styles.label}>Select Shielded Resource</label>
                  <select
                    style={styles.select}
                    value={selectedResource?.nonce || ''}
                    onChange={(e) => {
                      const resource = shieldedResources.find(r => r.nonce === e.target.value)
                      setSelectedResource(resource || null)
                    }}
                  >
                    <option value="">Select a resource...</option>
                    {shieldedResources.map((r, i) => (
                      <option key={i} value={r.nonce}>
                        Resource {i + 1} - Qty: {r.quantity}
                      </option>
                    ))}
                  </select>
                </div>

                <div style={styles.inputGroup}>
                  <label style={styles.label}>Recipient Address</label>
                  <input
                    type="text"
                    value={address || ''}
                    readOnly
                    style={{...styles.input, opacity: 0.7}}
                  />
                </div>

                <button
                  onClick={handleUnshield}
                  disabled={!selectedResource || !keypair || isLoading || backendStatus !== 'online' || txState !== 'idle'}
                  style={styles.button}
                >
                  {txState === 'pending' ? 'Starting Proof...' :
                   txState === 'generating' ? 'Generating ZK Proof (~7 min)...' :
                   txState === 'generating_proof' ? 'Generating Proof...' :
                   txState === 'awaiting_signature' ? 'Confirm in Wallet...' :
                   txState === 'confirming' ? 'Confirming...' :
                   isLoading ? 'Creating Proof...' :
                   'Unshield Tokens'}
                </button>
              </div>
            )}

            {/* Approval transaction success */}
            {isSuccess && txHash && (
              <div style={styles.successBox}>
                Approval confirmed! View on{' '}
                <a
                  href={`https://sepolia.etherscan.io/tx/${txHash}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  style={styles.link}
                >
                  Etherscan
                </a>
              </div>
            )}

            {/* Proof transaction status */}
            {txState !== 'idle' && (
              <div style={{
                ...styles.infoBox,
                background: txState === 'success' ? 'rgba(34, 197, 94, 0.1)' :
                           txState === 'error' ? 'rgba(239, 68, 68, 0.1)' :
                           'rgba(102, 126, 234, 0.1)',
                borderColor: txState === 'success' ? 'rgba(34, 197, 94, 0.3)' :
                            txState === 'error' ? 'rgba(239, 68, 68, 0.3)' :
                            'rgba(102, 126, 234, 0.2)',
                marginTop: '1rem',
              }}>
                {txState === 'generating_proof' && (
                  <div>Generating ZK proof... This may take a few minutes.</div>
                )}
                {txState === 'awaiting_signature' && (
                  <div>Please confirm the transaction in your wallet...</div>
                )}
                {txState === 'confirming' && (
                  <div>
                    Transaction sent! Waiting for confirmation...
                    {proofTxHash && (
                      <div style={{ marginTop: '0.5rem' }}>
                        <a
                          href={`https://sepolia.etherscan.io/tx/${proofTxHash}`}
                          target="_blank"
                          rel="noopener noreferrer"
                          style={styles.link}
                        >
                          View on Etherscan
                        </a>
                      </div>
                    )}
                  </div>
                )}
                {txState === 'success' && (
                  <div style={{ color: '#22c55e' }}>
                    Transaction confirmed!{' '}
                    {proofTxHash && (
                      <a
                        href={`https://sepolia.etherscan.io/tx/${proofTxHash}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        style={styles.link}
                      >
                        View on Etherscan
                      </a>
                    )}
                    <button onClick={resetTxState} style={{ ...styles.smallButton, marginLeft: '1rem' }}>
                      Dismiss
                    </button>
                  </div>
                )}
                {txState === 'error' && (
                  <div style={{ color: '#ef4444' }}>
                    Transaction failed. Please try again.
                    <button onClick={resetTxState} style={{ ...styles.smallButton, marginLeft: '1rem' }}>
                      Dismiss
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Shielded Resources List */}
          {shieldedResources.length > 0 && (
            <div style={styles.resourcesList}>
              <h3 style={styles.sectionTitle}>Your Shielded Resources</h3>
              {shieldedResources.map((r, i) => (
                <div key={i} style={styles.resourceItem}>
                  <span>Resource {i + 1}</span>
                  <span>Quantity: {r.quantity}</span>
                  <span style={styles.resourceNonce}>Nonce: {r.nonce.slice(0, 16)}...</span>
                </div>
              ))}
              <button
                onClick={() => { api.clearResources(); setShieldedResources([]) }}
                style={styles.smallButton}
              >
                Clear All Resources
              </button>
            </div>
          )}

          {/* Contract Info */}
          <div style={styles.contractInfo}>
            <h3 style={styles.contractTitle}>Deployed Contracts (Sepolia)</h3>
            <div style={styles.contractGrid}>
              <ContractLink name="Protocol Adapter" address={CONTRACTS.PROTOCOL_ADAPTER} />
              <ContractLink name="WETH Forwarder" address={CONTRACTS.WETH_FORWARDER} />
              <ContractLink name="USDC Forwarder" address={CONTRACTS.USDC_FORWARDER} />
              <ContractLink name="Uniswap Forwarder" address={CONTRACTS.UNISWAP_FORWARDER} />
            </div>
          </div>
        </main>
      ) : (
        <div style={styles.connectPrompt}>
          <p>Connect your wallet to get started</p>
        </div>
      )}

      <footer style={styles.footer}>
        <p>Built with Anoma Protocol Adapter + Uniswap V3</p>
        <p style={styles.footerNote}>
          Backend API: {api.API_BASE_URL}
        </p>
      </footer>
    </div>
  )
}

function ContractLink({ name, address }: { name: string; address: string }) {
  return (
    <div style={styles.contractItem}>
      <span style={styles.contractName}>{name}</span>
      <a
        href={`https://sepolia.etherscan.io/address/${address}`}
        target="_blank"
        rel="noopener noreferrer"
        style={styles.contractAddress}
      >
        {address.slice(0, 6)}...{address.slice(-4)}
      </a>
    </div>
  )
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    minHeight: '100vh',
    display: 'flex',
    flexDirection: 'column',
  },
  header: {
    padding: '2rem',
    textAlign: 'center',
    borderBottom: '1px solid rgba(255,255,255,0.1)',
  },
  title: {
    fontSize: '2.5rem',
    fontWeight: 'bold',
    background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
    WebkitBackgroundClip: 'text',
    WebkitTextFillColor: 'transparent',
    marginBottom: '0.5rem',
  },
  subtitle: {
    color: '#888',
    marginBottom: '1.5rem',
  },
  connectWrapper: {
    display: 'flex',
    justifyContent: 'center',
  },
  statusBar: {
    marginTop: '1rem',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '0.5rem',
    fontSize: '0.85rem',
    color: '#888',
  },
  statusDot: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
  },
  main: {
    flex: 1,
    padding: '2rem',
    maxWidth: '600px',
    margin: '0 auto',
    width: '100%',
  },
  keypairSection: {
    background: 'rgba(255,255,255,0.03)',
    borderRadius: '12px',
    padding: '1rem',
    marginBottom: '1.5rem',
  },
  sectionTitle: {
    fontSize: '0.9rem',
    color: '#888',
    marginBottom: '0.75rem',
  },
  keypairInfo: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: '1rem',
  },
  keyDisplay: {
    display: 'flex',
    alignItems: 'center',
    gap: '0.5rem',
  },
  keyLabel: {
    color: '#666',
    fontSize: '0.85rem',
  },
  keyValue: {
    fontSize: '0.8rem',
    color: '#667eea',
    background: 'rgba(102, 126, 234, 0.1)',
    padding: '0.25rem 0.5rem',
    borderRadius: '4px',
  },
  generateButton: {
    width: '100%',
    padding: '0.75rem',
    borderRadius: '8px',
    border: 'none',
    background: 'rgba(102, 126, 234, 0.2)',
    color: '#667eea',
    cursor: 'pointer',
  },
  smallButton: {
    padding: '0.5rem 1rem',
    borderRadius: '6px',
    border: 'none',
    background: 'rgba(255,255,255,0.1)',
    color: '#888',
    cursor: 'pointer',
    fontSize: '0.8rem',
  },
  tabs: {
    display: 'flex',
    gap: '0.5rem',
    marginBottom: '1.5rem',
  },
  tab: {
    flex: 1,
    padding: '0.75rem',
    border: 'none',
    borderRadius: '8px',
    background: 'rgba(255,255,255,0.05)',
    color: '#888',
    cursor: 'pointer',
    fontSize: '0.9rem',
    transition: 'all 0.2s',
  },
  tabActive: {
    background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
    color: 'white',
  },
  card: {
    background: 'rgba(255,255,255,0.05)',
    borderRadius: '16px',
    padding: '1.5rem',
    marginBottom: '1.5rem',
  },
  inputGroup: {
    marginBottom: '1rem',
  },
  label: {
    display: 'block',
    marginBottom: '0.5rem',
    color: '#888',
    fontSize: '0.9rem',
  },
  select: {
    width: '100%',
    padding: '0.75rem',
    borderRadius: '8px',
    border: '1px solid rgba(255,255,255,0.1)',
    background: 'rgba(0,0,0,0.3)',
    color: 'white',
    fontSize: '1rem',
  },
  input: {
    width: '100%',
    padding: '0.75rem',
    borderRadius: '8px',
    border: '1px solid rgba(255,255,255,0.1)',
    background: 'rgba(0,0,0,0.3)',
    color: 'white',
    fontSize: '1rem',
  },
  balance: {
    marginTop: '0.5rem',
    fontSize: '0.85rem',
    color: '#888',
  },
  tabContent: {
    marginTop: '1rem',
  },
  description: {
    color: '#aaa',
    fontSize: '0.9rem',
    marginBottom: '1rem',
    lineHeight: 1.5,
  },
  infoBox: {
    background: 'rgba(102, 126, 234, 0.1)',
    border: '1px solid rgba(102, 126, 234, 0.2)',
    borderRadius: '8px',
    padding: '0.75rem',
    marginBottom: '1rem',
    fontSize: '0.9rem',
  },
  warningText: {
    color: '#f59e0b',
    fontSize: '0.9rem',
    textAlign: 'center',
  },
  button: {
    width: '100%',
    padding: '1rem',
    borderRadius: '12px',
    border: 'none',
    background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
    color: 'white',
    fontSize: '1rem',
    fontWeight: 'bold',
    cursor: 'pointer',
    transition: 'transform 0.2s, opacity 0.2s',
  },
  errorBox: {
    marginBottom: '1rem',
    padding: '1rem',
    background: 'rgba(239, 68, 68, 0.1)',
    border: '1px solid rgba(239, 68, 68, 0.3)',
    borderRadius: '8px',
    color: '#ef4444',
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  dismissButton: {
    background: 'none',
    border: 'none',
    color: '#ef4444',
    cursor: 'pointer',
    fontSize: '1.2rem',
  },
  successBox: {
    marginTop: '1rem',
    padding: '1rem',
    background: 'rgba(34, 197, 94, 0.1)',
    border: '1px solid rgba(34, 197, 94, 0.3)',
    borderRadius: '8px',
    color: '#22c55e',
  },
  link: {
    color: '#667eea',
    textDecoration: 'underline',
  },
  resourcesList: {
    background: 'rgba(255,255,255,0.02)',
    borderRadius: '12px',
    padding: '1rem',
    marginBottom: '1.5rem',
  },
  resourceItem: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: '0.75rem',
    background: 'rgba(255,255,255,0.05)',
    borderRadius: '8px',
    marginBottom: '0.5rem',
    fontSize: '0.85rem',
  },
  resourceNonce: {
    color: '#666',
    fontSize: '0.75rem',
  },
  contractInfo: {
    background: 'rgba(255,255,255,0.02)',
    borderRadius: '12px',
    padding: '1rem',
  },
  contractTitle: {
    fontSize: '0.9rem',
    color: '#888',
    marginBottom: '0.75rem',
  },
  contractGrid: {
    display: 'grid',
    gridTemplateColumns: '1fr 1fr',
    gap: '0.5rem',
  },
  contractItem: {
    display: 'flex',
    flexDirection: 'column',
    gap: '0.25rem',
  },
  contractName: {
    fontSize: '0.75rem',
    color: '#666',
  },
  contractAddress: {
    fontSize: '0.8rem',
    color: '#667eea',
    textDecoration: 'none',
  },
  connectPrompt: {
    flex: 1,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: '#666',
  },
  footer: {
    padding: '1.5rem',
    textAlign: 'center',
    borderTop: '1px solid rgba(255,255,255,0.1)',
    color: '#666',
    fontSize: '0.85rem',
  },
  footerNote: {
    marginTop: '0.5rem',
    fontSize: '0.75rem',
    color: '#555',
  },
}

export default App
