import { Navigate, Route, Routes } from 'react-router-dom'
import { ProtectedRoute } from './protected-route'
import { WorkspaceLayout } from './workspace-layout'
import { LoginPage } from '@/features/auth/login-page'
import { HomePage } from './pages/home-page'
import { InboxPage } from './pages/inbox-page'
import { NotFoundPage } from './pages/not-found-page'
import { ProjectWorkspacePage } from './pages/project-workspace-page'
import { ProjectsPage } from './pages/projects-page'
import { KnowledgePage } from '@/features/knowledge'
import { AgentJobPage, AgentsOverviewPage } from '@/features/agents'
import { BackupSettingsPage } from '@/features/backup'

export function AppRoutes() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />

      <Route element={<ProtectedRoute />}>
        <Route element={<WorkspaceLayout />}>
          <Route index element={<HomePage />} />
          <Route path="inbox" element={<InboxPage />} />
          <Route path="projects" element={<ProjectsPage />} />
          <Route path="projects/:projectId/:viewId?" element={<ProjectWorkspacePage />} />
          <Route path="knowledge/*" element={<KnowledgePage />} />
          <Route path="agents" element={<AgentsOverviewPage />} />
          <Route path="agents/:jobId" element={<AgentJobPage />} />
          <Route path="settings/backup" element={<BackupSettingsPage />} />
        </Route>
      </Route>

      <Route path="/home" element={<Navigate to="/" replace />} />
      <Route path="*" element={<NotFoundPage />} />
    </Routes>
  )
}
