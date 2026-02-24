import React from 'react'
import { createRoot } from 'react-dom/client'

import '@borg/ui'
import './styles.css'
import { OnboardApp } from './App'

createRoot(document.getElementById('app')!).render(
  <React.StrictMode>
    <OnboardApp />
  </React.StrictMode>,
)
