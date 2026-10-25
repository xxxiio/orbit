import { useMemo, useState, type ReactNode } from 'react'
import { CreateProjectDialog } from '@/features/projects/create-project-dialog'
import { CreateTaskDialog } from '@/features/tasks'
import { WorkspaceActionsContext, type WorkspaceActionsValue } from '@/lib/workspace-actions'

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
