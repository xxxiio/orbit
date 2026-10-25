import { api } from '../client'
import { withQuery } from '../http'
import type {
  CreateFolderRequest,
  CreateFolderResponse,
  KnowledgeDocumentInfo,
  KnowledgeFileRelations,
  KnowledgeNode,
  MarkdownByIdResponse,
  MarkdownCommandRequest,
  MarkdownDocumentResponse,
  MarkdownManifestEntry,
  MarkdownReceipt,
  MoveFolderRequest,
  MoveFolderResponse,
} from '../contracts'

const BASE = '/authoritative'

/** Directory tree of the Markdown vault; folders are nodes even when empty. */
export function fetchKnowledgeTree(signal?: AbortSignal): Promise<KnowledgeNode> {
  return api.get<KnowledgeNode>(`${BASE}/knowledge-tree`, { signal })
}

/** Identified by vault-relative path, not by id. `path` must end in `.md`. */
export function fetchMarkdownDocument(
  path: string,
  signal?: AbortSignal,
): Promise<MarkdownDocumentResponse> {
  return api.get<MarkdownDocumentResponse>(withQuery(`${BASE}/markdown-document`, { path }), {
    signal,
  })
}

export function fetchMarkdownById(id: string, signal?: AbortSignal): Promise<MarkdownByIdResponse> {
  return api.get<MarkdownByIdResponse>(withQuery(`${BASE}/markdown-by-id`, { id }), { signal })
}

/** Sorted by id. */
export function fetchMarkdownInventory(signal?: AbortSignal): Promise<KnowledgeDocumentInfo[]> {
  return api.get<KnowledgeDocumentInfo[]>(`${BASE}/markdown-inventory`, { signal })
}

/** Inventory plus `body_sha256`, for verifying a mirror before publishing. */
export function fetchMarkdownManifest(signal?: AbortSignal): Promise<MarkdownManifestEntry[]> {
  return api.get<MarkdownManifestEntry[]>(`${BASE}/markdown-manifest`, { signal })
}

export function searchMarkdown(
  query: string,
  limit: number,
  signal?: AbortSignal,
): Promise<KnowledgeDocumentInfo[]> {
  return api.post<KnowledgeDocumentInfo[]>(`${BASE}/markdown-search`, { query, limit }, { signal })
}

export function fetchMarkdownRelations(
  id: string,
  signal?: AbortSignal,
): Promise<KnowledgeFileRelations> {
  return api.get<KnowledgeFileRelations>(withQuery(`${BASE}/markdown-relations`, { id }), { signal })
}

/**
 * Writes to disk first and only then accepts the receipt, so an error response
 * can still mean "the file changed". Callers must treat failures as `uncertain`
 * and re-read the document/manifest before retrying with a fresh command id.
 */
export function sendMarkdownCommand(request: MarkdownCommandRequest): Promise<MarkdownReceipt> {
  return api.post<MarkdownReceipt>(`${BASE}/markdown-commands`, request)
}

export function createFolder(request: CreateFolderRequest): Promise<CreateFolderResponse> {
  return api.post<CreateFolderResponse>(`${BASE}/folder-creates`, request)
}

export function moveFolder(request: MoveFolderRequest): Promise<MoveFolderResponse> {
  return api.post<MoveFolderResponse>(`${BASE}/folder-moves`, request)
}
