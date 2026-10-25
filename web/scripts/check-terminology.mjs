#!/usr/bin/env node
/**
 * Terminology guard for user-facing copy.
 *
 * User-facing language is "Workflow". The legacy feature name must not appear in
 * rendered text, labels, messages, metadata, or styles.
 *
 * What counts as user-visible:
 *   - TypeScript, JSX, HTML, and CSS files under src (string literals, JSX text,
 *     metadata, and copy)
 *   - index.html
 *
 * What is intentionally allowed:
 *   - comments (an internal note may reference the old name)
 *   - internal identifiers: snake_case, camelCase, SCREAMING_CASE
 *   - API path segments such as /v1/authoritative/<legacy-plural>
 *
 * This is a deliberately small, explainable check. Extend ALLOWLIST_PATTERNS when
 * an internal identifier is legitimate; do not silence a finding that is actually
 * rendered to a person.
 */
import { readdir, readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const TARGET_DIRS = ['src']
const TARGET_FILES = ['index.html']
const EXTENSIONS = new Set(['.ts', '.tsx', '.html', '.css'])

// Standalone word only: "blueprint" / "blueprints", case-insensitive.
const LEGACY_WORD = /\bblueprints?\b/gi

const ALLOWLIST_PATTERNS = [
  // Internal data and code identifiers.
  /\bblueprint_(?:id|ids|version|versions|ref|name|kind|type)\b/i,
  /\bblueprints?_[a-z0-9_]+\b/i,
  /\bblueprint[A-Z][A-Za-z0-9]*\b/,
  // HTTP paths and route segments.
  /\/blueprints?\b/i,
  /\bblueprints?\/[\w-]+/i,
]

const verbose = process.argv.includes('--verbose')

async function collectFiles(dir) {
  const files = []
  let entries
  try {
    entries = await readdir(dir, { withFileTypes: true })
  } catch {
    return files
  }

  for (const entry of entries) {
    if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name === 'coverage') continue
    const fullPath = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      files.push(...(await collectFiles(fullPath)))
    } else if (EXTENSIONS.has(path.extname(entry.name))) {
      files.push(fullPath)
    }
  }

  return files
}

function isComment(line, index) {
  const trimmed = line.trimStart()
  if (
    trimmed.startsWith('//') ||
    trimmed.startsWith('/*') ||
    trimmed.startsWith('*') ||
    trimmed.startsWith('<!--')
  ) {
    return true
  }

  const before = line.slice(0, index)
  const lastBlockStart = before.lastIndexOf('/*')
  const lastBlockEnd = before.lastIndexOf('*/')
  if (lastBlockStart > lastBlockEnd) return true
  if (before.includes('//')) return true
  if (before.includes('<!--') && !before.includes('-->')) return true

  return false
}

function isIdentifierBoundary(line, start, end) {
  const before = line[start - 1] ?? ''
  const after = line[end] ?? ''
  return /[A-Za-z0-9_$]/.test(before) || /[A-Za-z0-9_$]/.test(after)
}

function isAllowed(line, start, end) {
  const context = line.slice(Math.max(0, start - 48), Math.min(line.length, end + 48))
  return ALLOWLIST_PATTERNS.some((pattern) => pattern.test(context))
}

function scanContent(content, filePath) {
  const findings = []
  const lines = content.split(/\r?\n/)

  lines.forEach((line, lineIndex) => {
    LEGACY_WORD.lastIndex = 0
    let match
    while ((match = LEGACY_WORD.exec(line)) !== null) {
      const start = match.index
      const end = start + match[0].length
      if (isComment(line, start)) continue
      if (isIdentifierBoundary(line, start, end)) continue
      if (isAllowed(line, start, end)) continue

      findings.push({
        file: path.relative(ROOT, filePath),
        line: lineIndex + 1,
        column: start + 1,
        term: match[0],
        excerpt: line.trim(),
      })
    }
  })

  return findings
}

async function main() {
  const files = []

  for (const dir of TARGET_DIRS) {
    files.push(...(await collectFiles(path.join(ROOT, dir))))
  }
  for (const file of TARGET_FILES) {
    files.push(path.join(ROOT, file))
  }

  const uniqueFiles = [...new Set(files)].sort()
  const allFindings = []

  for (const file of uniqueFiles) {
    const content = await readFile(file, 'utf8')
    if (verbose) console.log(`checked ${path.relative(ROOT, file)}`)
    allFindings.push(...scanContent(content, file))
  }

  if (allFindings.length > 0) {
    console.error('Terminology check failed: use "Workflow" in user-facing copy.\n')
    for (const finding of allFindings) {
      console.error(`  ${finding.file}:${finding.line}:${finding.column}  “${finding.term}”`)
      console.error(`    ${finding.excerpt}`)
    }
    console.error(`\n${allFindings.length} finding(s) in ${uniqueFiles.length} files scanned.`)
    console.error('Internal identifiers, API paths, and comments are allowlisted in scripts/check-terminology.mjs.')
    process.exit(1)
  }

  console.log(
    `Terminology check passed: ${uniqueFiles.length} files scanned, no user-visible legacy feature name.`,
  )
}

main().catch((error) => {
  console.error('Terminology check could not run:', error)
  process.exit(1)
})
