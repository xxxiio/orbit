import { Fragment, type ReactNode } from 'react'

/**
 * A deliberately small Markdown renderer.
 *
 * It renders React nodes — never HTML — so user text cannot inject markup.
 * Supported: headings, paragraphs, lists, code fences, inline code, bold,
 * italic and http(s) links. Anything else is shown as written.
 */

type Block =
  | { type: 'heading'; level: 1 | 2 | 3; text: string }
  | { type: 'paragraph'; lines: string[] }
  | { type: 'bullets'; items: string[] }
  | { type: 'ordered'; items: string[] }
  | { type: 'quote'; lines: string[] }
  | { type: 'code'; language: string | null; lines: string[] }
  | { type: 'rule' }

function parseBlocks(source: string): Block[] {
  const lines = source.replace(/\r\n/g, '\n').split('\n')
  const blocks: Block[] = []
  let index = 0

  while (index < lines.length) {
    const line = lines[index]

    if (line.trim().length === 0) {
      index += 1
      continue
    }

    const fence = line.trim().match(/^```(.*)$/)
    if (fence) {
      const codeLines: string[] = []
      index += 1
      while (index < lines.length && !lines[index].trim().startsWith('```')) {
        codeLines.push(lines[index])
        index += 1
      }
      index += 1
      blocks.push({ type: 'code', language: fence[1].trim() || null, lines: codeLines })
      continue
    }

    const heading = line.match(/^(#{1,3})\s+(.*)$/)
    if (heading) {
      blocks.push({
        type: 'heading',
        level: heading[1].length as 1 | 2 | 3,
        text: heading[2],
      })
      index += 1
      continue
    }

    if (/^\s*(---+|\*\*\*+)\s*$/.test(line)) {
      blocks.push({ type: 'rule' })
      index += 1
      continue
    }

    if (/^\s*[-*]\s+/.test(line)) {
      const items: string[] = []
      while (index < lines.length && /^\s*[-*]\s+/.test(lines[index])) {
        items.push(lines[index].replace(/^\s*[-*]\s+/, ''))
        index += 1
      }
      blocks.push({ type: 'bullets', items })
      continue
    }

    if (/^\s*\d+\.\s+/.test(line)) {
      const items: string[] = []
      while (index < lines.length && /^\s*\d+\.\s+/.test(lines[index])) {
        items.push(lines[index].replace(/^\s*\d+\.\s+/, ''))
        index += 1
      }
      blocks.push({ type: 'ordered', items })
      continue
    }

    if (/^\s*>\s?/.test(line)) {
      const quoteLines: string[] = []
      while (index < lines.length && /^\s*>\s?/.test(lines[index])) {
        quoteLines.push(lines[index].replace(/^\s*>\s?/, ''))
        index += 1
      }
      blocks.push({ type: 'quote', lines: quoteLines })
      continue
    }

    const paragraph: string[] = []
    while (index < lines.length && lines[index].trim().length > 0) {
      const candidate = lines[index]
      if (
        /^\s*[-*]\s+/.test(candidate) ||
        /^\s*\d+\.\s+/.test(candidate) ||
        /^\s*>\s?/.test(candidate) ||
        /^#{1,3}\s+/.test(candidate) ||
        candidate.trim().startsWith('```')
      ) {
        break
      }
      paragraph.push(candidate)
      index += 1
    }
    if (paragraph.length > 0) blocks.push({ type: 'paragraph', lines: paragraph })
  }

  return blocks
}

const INLINE_PATTERN = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\((https?:\/\/[^\s)]+)\)|\*[^*]+\*)/g

function renderInline(text: string): ReactNode[] {
  const nodes: ReactNode[] = []
  let lastIndex = 0
  let match: RegExpExecArray | null
  let key = 0

  INLINE_PATTERN.lastIndex = 0
  while ((match = INLINE_PATTERN.exec(text)) !== null) {
    if (match.index > lastIndex) nodes.push(text.slice(lastIndex, match.index))
    const token = match[0]
    key += 1

    if (token.startsWith('**')) {
      nodes.push(
        <strong key={key} className="font-semibold text-ink">
          {token.slice(2, -2)}
        </strong>,
      )
    } else if (token.startsWith('`')) {
      nodes.push(
        <code key={key} className="rounded-xs bg-surface-sunken px-1 py-0.5 font-mono text-[0.8125em] text-ink-soft">
          {token.slice(1, -1)}
        </code>,
      )
    } else if (token.startsWith('[')) {
      const link = token.match(/^\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)$/)
      if (link) {
        nodes.push(
          <a
            key={key}
            href={link[2]}
            target="_blank"
            rel="noreferrer noopener"
            className="text-accent underline decoration-accent/40 underline-offset-2 hover:decoration-accent"
          >
            {link[1]}
          </a>,
        )
      } else {
        nodes.push(token)
      }
    } else {
      nodes.push(
        <em key={key} className="italic">
          {token.slice(1, -1)}
        </em>,
      )
    }
    lastIndex = match.index + token.length
  }

  if (lastIndex < text.length) nodes.push(text.slice(lastIndex))
  return nodes
}

function renderLines(lines: string[], separator: ReactNode): ReactNode {
  return lines.map((line, index) => (
    <Fragment key={index}>
      {index > 0 ? separator : null}
      {renderInline(line)}
    </Fragment>
  ))
}

export function Markdown({ source, className }: { source: string; className?: string }) {
  const blocks = parseBlocks(source)

  if (blocks.length === 0) {
    return <p className="text-sm italic text-ink-faint">No description yet.</p>
  }

  return (
    <div className={className}>
      {blocks.map((block, index) => {
        switch (block.type) {
          case 'heading': {
            const Tag = block.level === 1 ? 'h3' : block.level === 2 ? 'h4' : 'h5'
            return (
              <Tag
                key={index}
                className={
                  block.level === 1
                    ? 'mt-4 font-sans text-lg font-medium text-ink first:mt-0'
                    : 'mt-4 text-sm font-semibold text-ink first:mt-0'
                }
              >
                {renderInline(block.text)}
              </Tag>
            )
          }
          case 'paragraph':
            return (
              <p key={index} className="mt-2 text-sm leading-6 text-ink-soft first:mt-0">
                {renderLines(block.lines, ' ')}
              </p>
            )
          case 'bullets':
            return (
              <ul key={index} className="mt-2 list-disc space-y-1 pl-5 text-sm leading-6 text-ink-soft">
                {block.items.map((item, itemIndex) => (
                  <li key={itemIndex}>{renderInline(item)}</li>
                ))}
              </ul>
            )
          case 'ordered':
            return (
              <ol key={index} className="mt-2 list-decimal space-y-1 pl-5 text-sm leading-6 text-ink-soft">
                {block.items.map((item, itemIndex) => (
                  <li key={itemIndex}>{renderInline(item)}</li>
                ))}
              </ol>
            )
          case 'quote':
            return (
              <blockquote
                key={index}
                className="mt-2 border-l-2 border-accent/40 pl-3 text-sm italic leading-6 text-ink-muted"
              >
                {renderLines(block.lines, ' ')}
              </blockquote>
            )
          case 'code':
            return (
              <pre
                key={index}
                className="scrollbar-slim mt-2 overflow-x-auto rounded-md border border-line bg-surface-sunken p-3 font-mono text-xs leading-5 text-ink-soft"
              >
                <code>{block.lines.join('\n')}</code>
              </pre>
            )
          case 'rule':
            return <hr key={index} className="my-4" />
        }
      })}
    </div>
  )
}
