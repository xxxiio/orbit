/**
 * Verification report rendering shared by the history row action, the inspect
 * drawer and a restore preflight.
 *
 * The report answers one question: "is this archive internally sound and
 * compatible with this build?" It never says a restore happened or that the
 * recovered application works, and the copy below says so explicitly.
 */
import { Badge } from '@/components/ui/badge'
import { AlertCircleIcon, AlertTriangleIcon, CheckCircleIcon, DotIcon } from '@/components/ui/icons'
import { formatBytes } from './backup-model'
import type { VerificationCheck, VerificationReport } from '@/api/contracts'

function checkTone(status: VerificationCheck['status']): 'success' | 'danger' | 'neutral' {
  if (status === 'pass') return 'success'
  if (status === 'fail') return 'danger'
  return 'neutral'
}

export function VerificationReportView({
  report,
  className,
}: {
  report: VerificationReport
  className?: string
}) {
  return (
    <div className={className} data-verification-ok={report.ok ? 'true' : 'false'}>
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={report.ok ? 'success' : 'danger'} dot>
          {report.ok ? 'Verification passed' : 'Verification failed'}
        </Badge>
        <span className="text-xs text-ink-muted">
          {report.verified_files.length} file
          {report.verified_files.length === 1 ? '' : 's'} checked · format v{report.format_version}
          {report.schema_version !== null ? ` · schema ${report.schema_version}` : ''}
          {report.archive_bytes > 0 ? ` · ${formatBytes(report.archive_bytes)}` : ''}
        </span>
      </div>

      {report.problems.length > 0 ? (
        <ul aria-label="Verification problems" className="mt-3 flex flex-col gap-1.5">
          {report.problems.map((problem, index) => (
            <li
              key={`${problem.check}-${index}`}
              className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink"
            >
              <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
              <span className="min-w-0 flex-1">
                <span className="font-medium">{problem.check}:</span> {problem.detail}
              </span>
            </li>
          ))}
        </ul>
      ) : null}

      {report.warnings.length > 0 ? (
        <ul aria-label="Verification warnings" className="mt-3 flex flex-col gap-1.5">
          {report.warnings.map((warning, index) => (
            <li
              key={index}
              className="flex items-start gap-1.5 text-xs leading-5 text-warning-ink"
            >
              <AlertTriangleIcon size={13} className="mt-0.5 shrink-0" />
              <span className="min-w-0 flex-1">{warning}</span>
            </li>
          ))}
        </ul>
      ) : null}

      {report.checks.length > 0 ? (
        <ul aria-label="Verification checks" className="mt-3 flex flex-col gap-1">
          {report.checks.map((check) => (
            <li key={check.name} className="flex items-start gap-2 text-xs leading-5">
              {check.status === 'pass' ? (
                <CheckCircleIcon size={13} className="mt-0.5 shrink-0 text-success" />
              ) : check.status === 'fail' ? (
                <AlertCircleIcon size={13} className="mt-0.5 shrink-0 text-danger" />
              ) : (
                <DotIcon size={13} className="mt-0.5 shrink-0 text-ink-faint" />
              )}
              <span className="min-w-0 flex-1">
                <span className="font-medium text-ink">{check.name}</span>
                <span className="text-ink-muted"> — {check.detail}</span>
                {check.checked !== undefined ? (
                  <span className="text-ink-faint"> ({check.checked} checked)</span>
                ) : null}
              </span>
              <Badge tone={checkTone(check.status)}>{check.status}</Badge>
            </li>
          ))}
        </ul>
      ) : null}

      <p className="mt-3 text-2xs leading-4 text-ink-faint">
        Verification is not restoration. A verified archive can still fail to become a working
        application — for example when an external agent session it referred to is gone.
      </p>
    </div>
  )
}
