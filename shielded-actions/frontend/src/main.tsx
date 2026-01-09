import React from 'react'
import ReactDOM from 'react-dom/client'
import { WagmiProvider } from 'wagmi'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RainbowKitProvider, getDefaultConfig, darkTheme } from '@rainbow-me/rainbowkit'
import { sepolia } from 'wagmi/chains'
import '@rainbow-me/rainbowkit/styles.css'
import App from './App'

const config = getDefaultConfig({
  appName: 'Shielded Actions',
  projectId: 'shielded-actions-demo',
  chains: [sepolia],
})

const queryClient = new QueryClient()

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    {/* @ts-expect-error - RainbowKit types mismatch with wagmi v3 */}
    <WagmiProvider config={config}>
      <QueryClientProvider client={queryClient}>
        <RainbowKitProvider theme={darkTheme()}>
          <App />
        </RainbowKitProvider>
      </QueryClientProvider>
    </WagmiProvider>
  </React.StrictMode>
)
