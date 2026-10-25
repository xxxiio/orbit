import { useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { useProjects } from '@/hooks'
import type { Command } from '@/components/ui/command-palette'
import { FolderIcon, HomeIcon, InboxIcon, PlusIcon } from '@/components/ui/icons'
import type { ProjectNavItem } from '@/components/navigation/nav'
import { AppShell } from './app-shell'
import { WorkspaceActionsProvider, useWorkspaceActions } from './workspace-actions'
import { TaskDetailDrawer } from '@/features/tasks/task-drawer'
import { useRecentProjectIds } from '@/features/projects/recent-projects'

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
    />
  )
}

export function WorkspaceLayout() {
  return (
    <WorkspaceActionsProvider>
      <WorkspaceShell />
      <TaskDetailDrawer />
    </WorkspaceActionsProvider>
  )
}
