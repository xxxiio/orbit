import { useCallback, useEffect, useMemo, useState } from 'react'
import type { Task } from '@/api/adapters'

/**
 * Selection is shared by every task list, including the `x` keyboard toggle.
 *
 * `tasks` prunes stale ids when the underlying list changes. `filtered` is the
 * set bulk commands act on. `visible` is what "select all" covers: on a paged
 * list that is the current page, never rows hidden behind pagination.
 */
export function useTaskSelection({
  tasks,
  filtered,
  visible,
}: {
  tasks: Task[]
  filtered: Task[]
  visible: Task[]
}) {
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set())

  useEffect(() => {
    const valid = new Set(tasks.map((task) => task.id))
    setSelected((current) => {
      const next = new Set([...current].filter((id) => valid.has(id)))
      return next.size === current.size ? current : next
    })
  }, [tasks])

  const toggleSelect = useCallback((taskId: string) => {
    setSelected((current) => {
      const next = new Set(current)
      if (next.has(taskId)) next.delete(taskId)
      else next.add(taskId)
      return next
    })
  }, [])

  const toggleAll = useCallback(() => {
    setSelected((current) => {
      const all = visible.every((task) => current.has(task.id))
      if (all) {
        const next = new Set(current)
        for (const task of visible) next.delete(task.id)
        return next
      }
      return new Set([...current, ...visible.map((task) => task.id)])
    })
  }, [visible])

  const clear = useCallback(() => setSelected(new Set()), [])

  const selectedTasks = useMemo(
    () => filtered.filter((task) => selected.has(task.id)),
    [filtered, selected],
  )

  return { selected, selectedTasks, toggleSelect, toggleAll, clear }
}
