import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter } from 'react-router-dom'
import * as TooltipPrimitive from '@radix-ui/react-tooltip'
import { ApiError } from '@/api/client'
import { AppErrorBoundary } from '@/components/feedback/error-boundary'
import { ToastProvider } from '@/components/ui/toast'
import { AppRoutes } from './routes'
import { ThemeProvider } from './theme'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      refetchOnWindowFocus: false,
      retry: (failureCount, error) => {
        if (error instanceof ApiError && error.status >= 400 && error.status < 500) {
          return false
        }
        return failureCount < 1
      },
    },
    mutations: {
      retry: false,
    },
  },
})

export function App() {
  return (
    <AppErrorBoundary>
      <ThemeProvider>
        <QueryClientProvider client={queryClient}>
          <ToastProvider>
            <TooltipPrimitive.Provider delayDuration={350} skipDelayDuration={150}>
              <BrowserRouter>
                <AppRoutes />
              </BrowserRouter>
            </TooltipPrimitive.Provider>
          </ToastProvider>
        </QueryClientProvider>
      </ThemeProvider>
    </AppErrorBoundary>
  )
}
