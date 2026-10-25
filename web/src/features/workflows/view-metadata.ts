/**
 * Workflow view metadata — a compatibility barrel.
 *
 * The declarations moved into `./declarations/{filters,metrics,columns,sorts,
 * views,reserved}`. This module keeps every existing import path working for
 * one iteration; new code should import the concern it needs from
 * `@/features/workflows/declarations`.
 */
export * from './declarations'
