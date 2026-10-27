import { useMemo } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useProjects } from '@/hooks'
import type { Command } from '@/components/ui/command-palette'
import { FolderIcon, HomeIcon, InboxIcon, PlusIcon, BookIcon, AgentsIcon } from '@/components/ui/icons'
import type { ProjectNavItem } from '@/components/navigation/nav'
import { AppShell } from './app-shell'
import type { SidebarSession } from '@/components/layout/sidebar'
import { useSession, useSignOutAction } from '@/features/auth/use-auth'
import { WorkspaceActionsProvider } from './workspace-actions'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import { TaskDetailDrawer } from '@/features/tasks'
import { useRecentProjectIds } from '@/lib/recent-projects'
import { isKnowledgePath } from './route-scope'

/**
 * Binds the shell to live workspace data. Everything registered here is
 * implemented: the command list only contains navigation and the two creation
 * dialogs the pages also use.
 */
function WorkspaceShell() {
  const projects = useProjects()
  const recentIds = useRecentProjectIds()
  const navigate = useNavigate()
  const { openNewTask, openNewProject } = useWorkspaceActions()
  const auth = useSession()
  const { signOut, pending: signingOut } = useSignOutAction()

  const session = useMemo<SidebarSession>(
    () => ({
      deviceId: auth.session?.device_id,
      pending: auth.isLoading,
      error: Boolean(auth.error),
      signingOut,
      onSignOut: signOut,
    }),
    [auth.session?.device_id, auth.isLoading, auth.error, signOut, signingOut],
  )

  const orderedProjects = useMemo(() => {
    const rank = new Map(recentIds.map((id, index) => [id, index]))
    return [...projects].sort((left, right) => {
      const leftRank = rank.get(left.id)
      const rightRank = rank.get(right.id)
      if (leftRank !== undefined && rightRank !== undefined) return leftRank - rightRank
      if (leftRank !== undefined) return -1
      if (rightRank !== undefined) return 1
      return left.name.localeCompare(right.name)
    })
  }, [projects, recentIds])

  const navProjects: ProjectNavItem[] = useMemo(
    () =>
      orderedProjects.map((project) => ({
        id: project.id,
        name: project.name,
        recent: recentIds.slice(0, 3).includes(project.id),
      })),
    [orderedProjects, recentIds],
  )

  const commands: Command[] = useMemo(
    () => [
      {
        id: 'new-task',
        label: 'New task',
        hint: 'c',
        icon: <PlusIcon size={15} />,
        keywords: 'create capture add',
        onSelect: () => openNewTask(),
      },
      {
        id: 'new-project',
        label: 'New project',
        icon: <FolderIcon size={15} />,
        keywords: 'create start',
        onSelect: openNewProject,
      },
      {
        id: 'go-home',
        label: 'Go to Home',
        hint: 'g h',
        icon: <HomeIcon size={15} />,
        keywords: 'today overview',
        onSelect: () => navigate('/'),
      },
      {
        id: 'go-inbox',
        label: 'Go to Inbox',
        hint: 'g i',
        icon: <InboxIcon size={15} />,
        keywords: 'triage unassigned',
        onSelect: () => navigate('/inbox'),
      },
      {
        id: 'go-projects',
        label: 'Go to Projects',
        hint: 'g p',
        icon: <FolderIcon size={15} />,
        keywords: 'index all',
        onSelect: () => navigate('/projects'),
      },
      {
        id: 'go-knowledge',
        label: 'Go to Knowledge',
        icon: <BookIcon size={15} />,
        keywords: 'notes markdown vault',
        onSelect: () => navigate('/knowledge'),
      },
      {
        id: 'go-agents',
        label: 'Go to Agents',
        icon: <AgentsIcon size={15} />,
        keywords: 'jobs handoff review',
        onSelect: () => navigate('/agents'),
      },
      ...orderedProjects.map((project) => ({
        id: `open-project-${project.id}`,
        label: `Open project: ${project.name}`,
        icon: <FolderIcon size={15} />,
        keywords: `project ${project.name}`,
        onSelect: () => navigate(`/projects/${project.id}`),
      })),
    ],
    [navigate, openNewProject, openNewTask, orderedProjects],
  )

  return (
    <AppShell
      projects={navProjects}
      commands={commands}
      onNewProject={openNewProject}
      onNewTask={() => openNewTask()}
      session={session}
    />
  )
}

export function WorkspaceLayout() {
  const location = useLocation()
  /**
   * On a knowledge route `?task=` names the task a note is being linked to, not
   * an open task drawer. The drawer is not mounted there at all, so it cannot
   * cover the note's reference panel and there is no focus trap to escape. Every
   * other surface keeps the documented `?task=` drawer contract.
   */
  const taskDrawer = isKnowledgePath(location.pathname) ? null : <TaskDetailDrawer />
  return (
    <WorkspaceActionsProvider>
      <WorkspaceShell />
      {taskDrawer}
    </WorkspaceActionsProvider>
  )
}
