import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.jsx'
import { AuthProvider } from './context/AuthContext'
import { ThemeProvider } from './context/ThemeContext'

async function enableMocking() {
  if (import.meta.env.VITE_MOCK_API === 'true') {
    console.log('🚀 [Dev] Mocking enabled');
    const { worker } = await import('./mocks/browser')
    return worker.start({
      onUnhandledRequest: 'bypass',
    })
  } else {
    console.log('🌐 [Dev] Connecting to real backend');
  }
}

enableMocking().then(() => {
  createRoot(document.getElementById('root')).render(
    <StrictMode>
      <ThemeProvider>
        <AuthProvider>
          <App />
        </AuthProvider>
      </ThemeProvider>
    </StrictMode>,
  )
})
