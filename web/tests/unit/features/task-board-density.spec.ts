import { describe, expect, it } from 'vitest'
import {
  BOARD_DENSITY_STORAGE_KEY,
  readBoardDensity,
  writeBoardDensity,
} from '@/features/tasks/task-board'

function memoryStorage(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial))
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value)
    },
    read: (key: string) => values.get(key) ?? null,
  }
}

describe('board density preference', () => {
  it('is compact by default and only recognises comfortable', () => {
    expect(readBoardDensity(memoryStorage())).toBe('compact')
    expect(readBoardDensity(memoryStorage({ [BOARD_DENSITY_STORAGE_KEY]: 'comfortable' }))).toBe(
      'comfortable',
    )
    expect(readBoardDensity(memoryStorage({ [BOARD_DENSITY_STORAGE_KEY]: 'huge' }))).toBe('compact')
  })

  it('persists the choice without requiring storage to exist', () => {
    const storage = memoryStorage()
    writeBoardDensity('comfortable', storage)
    expect(storage.read(BOARD_DENSITY_STORAGE_KEY)).toBe('comfortable')
    writeBoardDensity('compact', storage)
    expect(storage.read(BOARD_DENSITY_STORAGE_KEY)).toBe('compact')

    // A blocked or absent storage is not an error; the board keeps working.
    expect(() => writeBoardDensity('comfortable', null)).not.toThrow()
    expect(readBoardDensity(null)).toBe('compact')
  })
})
