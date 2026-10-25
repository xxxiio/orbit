/**
 * The workflow declaration vocabulary, one module per concern.
 *
 * A view declaration contributes rows, chrome and copy; each concern lives in
 * its own module so a new workflow source adds a descriptor — not a branch in a
 * 900-line file. The barrel is the only import path other features need.
 */
export * from './filters'
export * from './metrics'
export * from './columns'
export * from './sorts'
export * from './views'
export * from './reserved'
