import { api } from '../client'
import type { MilestoneCommand, MilestoneResult, MilestoneSnapshot } from '../contracts'

const BASE = '/authoritative'

export function fetchMilestones(signal?: AbortSignal): Promise<MilestoneSnapshot> {
  return api.get<MilestoneSnapshot>(`${BASE}/milestones`, { signal })
}

export function sendMilestoneCommand(command: MilestoneCommand): Promise<MilestoneResult> {
  return api.post<MilestoneResult>(`${BASE}/milestone-commands`, command)
}
