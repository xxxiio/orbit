import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@fontsource-variable/inter'
import '@fontsource-variable/jetbrains-mono'
import './styles/tokens.css'
import './styles/globals.css'
import { App } from './app/App'
import { applyInitialTheme } from '@/components/theme/theme'

applyInitialTheme()

const container = document.getElementById('root')
if (!container) {
  throw new Error('Orbit web app root element is missing')
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
