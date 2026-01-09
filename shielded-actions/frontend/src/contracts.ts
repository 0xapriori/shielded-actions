// Deployed contract addresses on Sepolia
export const CONTRACTS = {
  PROTOCOL_ADAPTER: '0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525' as const,
  WETH_FORWARDER: '0xD5307D777dC60b763b74945BF5A42ba93ce44e4b' as const,
  USDC_FORWARDER: '0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE' as const,
  UNISWAP_FORWARDER: '0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA' as const,
  // External
  WETH: '0x7b79995e5f793A07Bc00c21412e50Ecae098E7f9' as const,
  USDC: '0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238' as const,
  UNISWAP_ROUTER: '0x3bFA4769FB09eefC5a80d6E87c3B9C650f7Ae48E' as const,
} as const

// ERC20 ABI for basic operations
export const ERC20_ABI = [
  {
    name: 'approve',
    type: 'function',
    stateMutability: 'nonpayable',
    inputs: [
      { name: 'spender', type: 'address' },
      { name: 'amount', type: 'uint256' }
    ],
    outputs: [{ type: 'bool' }]
  },
  {
    name: 'balanceOf',
    type: 'function',
    stateMutability: 'view',
    inputs: [{ name: 'account', type: 'address' }],
    outputs: [{ type: 'uint256' }]
  },
  {
    name: 'allowance',
    type: 'function',
    stateMutability: 'view',
    inputs: [
      { name: 'owner', type: 'address' },
      { name: 'spender', type: 'address' }
    ],
    outputs: [{ type: 'uint256' }]
  },
  {
    name: 'decimals',
    type: 'function',
    stateMutability: 'view',
    inputs: [],
    outputs: [{ type: 'uint8' }]
  },
  {
    name: 'symbol',
    type: 'function',
    stateMutability: 'view',
    inputs: [],
    outputs: [{ type: 'string' }]
  }
] as const

// ERC20Forwarder ABI
export const ERC20_FORWARDER_ABI = [
  {
    name: 'forwardCall',
    type: 'function',
    stateMutability: 'nonpayable',
    inputs: [
      { name: 'logicRef', type: 'bytes32' },
      { name: 'input', type: 'bytes' }
    ],
    outputs: [{ name: 'output', type: 'bytes' }]
  },
  {
    name: 'token',
    type: 'function',
    stateMutability: 'view',
    inputs: [],
    outputs: [{ type: 'address' }]
  },
  {
    name: 'getBalance',
    type: 'function',
    stateMutability: 'view',
    inputs: [],
    outputs: [{ type: 'uint256' }]
  },
  {
    name: 'protocolAdapter',
    type: 'function',
    stateMutability: 'view',
    inputs: [],
    outputs: [{ type: 'address' }]
  }
] as const

// Protocol Adapter ABI (partial - key functions only)
export const PROTOCOL_ADAPTER_ABI = [
  {
    name: 'execute',
    type: 'function',
    stateMutability: 'nonpayable',
    inputs: [
      {
        name: 'transaction',
        type: 'tuple',
        components: [
          {
            name: 'actions',
            type: 'tuple[]',
            components: [
              {
                name: 'logicVerifierInputs',
                type: 'tuple[]',
                components: [
                  { name: 'tag', type: 'bytes32' },
                  { name: 'verifyingKey', type: 'bytes32' },
                  {
                    name: 'appData',
                    type: 'tuple',
                    components: [
                      { name: 'discoveryPayload', type: 'tuple[]', components: [{ name: 'blob', type: 'bytes' }, { name: 'deletionCriterion', type: 'uint8' }] },
                      { name: 'resourcePayload', type: 'tuple[]', components: [{ name: 'blob', type: 'bytes' }, { name: 'deletionCriterion', type: 'uint8' }] },
                      { name: 'externalPayload', type: 'tuple[]', components: [{ name: 'blob', type: 'bytes' }, { name: 'deletionCriterion', type: 'uint8' }] },
                      { name: 'applicationPayload', type: 'tuple[]', components: [{ name: 'blob', type: 'bytes' }, { name: 'deletionCriterion', type: 'uint8' }] }
                    ]
                  },
                  { name: 'proof', type: 'bytes' }
                ]
              },
              {
                name: 'complianceVerifierInputs',
                type: 'tuple[]',
                components: [
                  { name: 'proof', type: 'bytes' },
                  {
                    name: 'instance',
                    type: 'tuple',
                    components: [
                      {
                        name: 'consumed',
                        type: 'tuple',
                        components: [
                          { name: 'nullifier', type: 'bytes32' },
                          { name: 'commitmentTreeRoot', type: 'bytes32' },
                          { name: 'logicRef', type: 'bytes32' }
                        ]
                      },
                      {
                        name: 'created',
                        type: 'tuple',
                        components: [
                          { name: 'commitment', type: 'bytes32' },
                          { name: 'logicRef', type: 'bytes32' }
                        ]
                      },
                      { name: 'unitDeltaX', type: 'bytes32' },
                      { name: 'unitDeltaY', type: 'bytes32' }
                    ]
                  }
                ]
              }
            ]
          },
          { name: 'deltaProof', type: 'bytes' },
          { name: 'aggregationProof', type: 'bytes' }
        ]
      }
    ],
    outputs: []
  },
  {
    name: 'isEmergencyStopped',
    type: 'function',
    stateMutability: 'view',
    inputs: [],
    outputs: [{ type: 'bool' }]
  },
  {
    name: 'getVersion',
    type: 'function',
    stateMutability: 'pure',
    inputs: [],
    outputs: [{ type: 'bytes32' }]
  }
] as const
