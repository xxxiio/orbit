import { createContext, useContext, useMemo, useState, type ReactNode } from 'react'
import { CreateProjectDialog } from '@/features/projects/create-project-dialog'
import { CreateTaskDialog } from '@/features/tasks/create-task-dialog'

interface WorkspaceActionsValue {
  /** Open the single New task dialog, optionally pinned to a project. */
  openNewTask: (projectId?: string) => void
  openNewProject: () => void
}

const WorkspaceActionsContext = createContext<WorkspaceActionsValue | null>(null)

export function useWorkspaceActions(): WorkspaceActionsValue {
  const value = useContext(WorkspaceActionsContext)
  if (!value) throw new Error('useWorkspaceActions must be used inside WorkspaceActionsProvider')
  return value
}

/**
 * One owner for the workspace-level creation dialogs, so the sidebar, the
 * command palette and every page open the same form with the same behaviour.
 */
export function WorkspaceActionsProvider({ children }: { children: ReactNode }) {
  const [taskDialog, setTaskDialog] = useState<{ open: boolean; projectId?: string }>({ open: false })
  const [projectDialogOpen, setProjectDialogOpen] = useState(false)

  const value = useMemo<WorkspaceActionsValue>(
    () => ({
      openNewTask: (projectId?: string) => setTaskDialog({ open: true, projectId }),
      openNewProject: () => setProjectDialogOpen(true),
    }),
    [],
  )

  return (
    <WorkspaceActionsContext.Provider value={value}>
      {children}
      <CreateTaskDialog
        open={taskDialog.open}
        defaultProjectId={taskDialog.projectId}
        onOpenChange={(open) => setTaskDialog((current) => ({ ...current, open }))}
      />
      <CreateProjectDialog
        open={projectDialogOpen}
        onOpenChange={setProjectDialogOpen}
      />
    </WorkspaceActionsContext.Provider>
  )
}
