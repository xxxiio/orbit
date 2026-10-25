/**
 * Knowledge commands. A write publishes to disk before the receipt, so an error
 * does not prove nothing changed: the disposition is `uncertain` and an
 * identical retry replays the same command id.
 */
import { queryKeys } from '../queries'
import type { MarkdownCommand, MarkdownReceipt } from '../contracts'
import { sendMarkdownCommand } from '../endpoints/knowledge'
import { useApiMutation, type CommandMutation } from './core'

export interface MarkdownCommandVariables {
  /**
   * Minted by the caller before the first attempt. A knowledge write publishes
   * to disk before the receipt, so an error can still mean the file changed;
   * the identical retry must replay this id, not mint a new one.
   */
  commandId: string
  command: MarkdownCommand
}

/**
 * Knowledge writes are published to disk before the receipt is accepted, so an
 * error does not prove nothing changed. Treat it as `uncertain`: re-read the
 * document, and if the bytes still match what the command expects, replay the
 * **same** command id (same id + same payload replays; a new id is never needed
 * while the outcome is unknown).
 */
export function useMarkdownCommand(): CommandMutation<MarkdownReceipt, MarkdownCommandVariables> {
  return useApiMutation<MarkdownReceipt, MarkdownCommandVariables>({
    uncertainOnError: true,
    mutationFn: ({ command, commandId }) =>
      sendMarkdownCommand({ command_id: commandId, command }),
    affectedKeys: () => [queryKeys.knowledgeAll()],
  })
}
