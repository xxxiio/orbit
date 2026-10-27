//! Durable capability installation, project-scoped grants, and action receipts.
//!
//! Installation, authorization, request and claim are deliberately separate
//! facts. A stored installation never grants execution authority by itself, and
//! a grant is scoped to exactly one project. The reference capability's only
//! durable effect is a note row keyed by the action command ID, so a replay can
//! never duplicate an effect.
use crate::blueprint_actions::BlueprintActionRequest;
use crate::{SqliteStore, repo_error};
use orbit_application::RepositoryError;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

const PERMISSIONS: [&str; 3] = ["read", "mutate", "external_effect"];
const OUTCOMES: [&str; 3] = ["succeeded", "failed", "uncertain"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityInstallation {
    pub capability_id: String,
    pub provider: String,
    pub permission: String,
    pub actions_json: String,
    pub requires_json: String,
    pub installed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityGrant {
    pub capability_id: String,
    pub project_id: String,
    pub granted_by: String,
    pub granted_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityReceipt {
    pub command_id: String,
    pub item_id: String,
    pub capability: String,
    pub action_id: String,
    pub provider: String,
    pub outcome: String,
    pub detail: Value,
    pub recorded_at_ms: i64,
}

/// One durable effect produced by the trusted reference capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceWorkItemNote {
    pub command_id: String,
    pub item_id: String,
    pub note: String,
    pub tag: Option<String>,
    pub created_at_ms: i64,
}

/// Everything one settlement commits, together, in a single savepoint: the
/// outcome, who reported it, the detail document and — when the capability
/// declares one — the durable effect.
struct Settlement<'a> {
    outcome: &'a str,
    provider: &'a str,
    detail: &'a Value,
    effect: Option<&'a CapabilityEffectRecord<'a>>,
    recorded_at_ms: i64,
    existing: Option<CapabilityReceipt>,
}

/// A declared, schema-checked effect to persist in the same savepoint as the
/// settlement that produced it.
///
/// It carries no table, path or provider: the storage layer already knows the
/// one effect table this build has, and the runtime has already validated the
/// payload against the capability's compile-time declaration. `command_id` is
/// the row's primary key, so one command can never produce two effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityEffectRecord<'a> {
    pub kind: &'a str,
    pub item_id: &'a str,
    pub note: &'a str,
    pub tag: Option<&'a str>,
}

/// A device's announcement that it can execute one capability.
///
/// Enrollment is the connected deployment's whole answer to "is there an
/// executor?" — it is a durable, auditable fact rather than a process-local
/// registry entry in the server. It grants nothing: installation and the
/// project grant are still separate durable facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityExecutorAnnouncement {
    pub device_id: String,
    pub capability_id: String,
    pub announced_at_ms: i64,
}

fn valid_ident(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'_'
        })
}

fn valid_command_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.'))
}

fn clamp_limit(value: usize) -> i64 {
    i64::try_from(value.clamp(1, 1_000)).unwrap_or(1_000)
}

impl SqliteStore {
    pub(crate) fn ensure_capability_install_schema(&self) -> Result<(), RepositoryError> {
        self.execute_batch(
            "CREATE TABLE IF NOT EXISTS orbit_capability_installations (
                capability_id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                permission TEXT NOT NULL CHECK(permission IN ('read','mutate','external_effect')),
                actions_json TEXT NOT NULL CHECK(json_valid(actions_json)),
                requires_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(requires_json)),
                installed_at_ms INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS orbit_capability_grants (
                capability_id TEXT NOT NULL,
                project_id TEXT NOT NULL,
                granted_by TEXT NOT NULL,
                granted_at_ms INTEGER NOT NULL,
                PRIMARY KEY(capability_id, project_id));
            CREATE INDEX IF NOT EXISTS orbit_capability_grants_project
                ON orbit_capability_grants(project_id, capability_id);
            CREATE TABLE IF NOT EXISTS orbit_blueprint_action_receipts (
                command_id TEXT PRIMARY KEY,
                item_id TEXT NOT NULL,
                capability TEXT NOT NULL,
                action_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                outcome TEXT NOT NULL CHECK(outcome IN ('succeeded','failed','uncertain')),
                detail_json TEXT NOT NULL CHECK(json_valid(detail_json)),
                recorded_at_ms INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS orbit_blueprint_action_receipts_item
                ON orbit_blueprint_action_receipts(item_id, recorded_at_ms);
            CREATE TABLE IF NOT EXISTS orbit_reference_work_item_notes (
                command_id TEXT PRIMARY KEY,
                item_id TEXT NOT NULL,
                note TEXT NOT NULL,
                tag TEXT,
                created_at_ms INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS orbit_reference_work_item_notes_item
                ON orbit_reference_work_item_notes(item_id, created_at_ms);
            CREATE TABLE IF NOT EXISTS orbit_capability_executors (
                device_id TEXT NOT NULL,
                capability_id TEXT NOT NULL,
                announced_at_ms INTEGER NOT NULL CHECK(announced_at_ms >= 0),
                PRIMARY KEY (device_id, capability_id));",
        )
        .map_err(repo_error)
    }

    /// Record that a device can execute a capability. Re-announcing is
    /// idempotent and refreshes the announcement time, so the roster reflects
    /// the most recent claim of ability rather than a stale one.
    pub fn announce_capability_executor(
        &mut self,
        device_id: &str,
        capability_id: &str,
        at_ms: i64,
    ) -> Result<CapabilityExecutorAnnouncement, RepositoryError> {
        if device_id.trim().is_empty()
            || device_id.len() > 128
            || !valid_ident(capability_id)
            || at_ms < 0
        {
            return Err(RepositoryError::Conflict);
        }
        // Enrollment only makes sense for a device the authority can still
        // authenticate; a revoked device must not gain an executor row.
        let active: bool = self
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM devices WHERE id=?1 AND revoked=0)",
                [device_id],
                |row| row.get(0),
            )
            .map_err(repo_error)?;
        if !active {
            return Err(RepositoryError::Conflict);
        }
        self.execute(
            "INSERT INTO orbit_capability_executors (device_id,capability_id,announced_at_ms)
             VALUES (?1,?2,?3)
             ON CONFLICT(device_id,capability_id) DO UPDATE SET announced_at_ms=?3",
            params![device_id, capability_id, at_ms],
        )
        .map_err(repo_error)?;
        self.query_row(
            "SELECT device_id,capability_id,announced_at_ms FROM orbit_capability_executors
             WHERE device_id=?1 AND capability_id=?2",
            params![device_id, capability_id],
            capability_executor_from_row,
        )
        .map_err(repo_error)
    }

    /// The roster of announcements held by devices that are still active,
    /// ordered by device then capability. A revoked device's row stays durable
    /// but is not reported, because it can no longer claim anything.
    pub fn capability_executor_announcements(
        &self,
    ) -> Result<Vec<CapabilityExecutorAnnouncement>, RepositoryError> {
        let mut statement = self
            .prepare(
                "SELECT e.device_id,e.capability_id,e.announced_at_ms
                 FROM orbit_capability_executors e
                 JOIN devices d ON d.id=e.device_id AND d.revoked=0
                 ORDER BY e.device_id, e.capability_id",
            )
            .map_err(repo_error)?;
        let rows = statement
            .query_map([], capability_executor_from_row)
            .map_err(repo_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }

    pub fn capability_executor_announcements_for(
        &self,
        capability_id: &str,
    ) -> Result<Vec<CapabilityExecutorAnnouncement>, RepositoryError> {
        let mut statement = self
            .prepare(
                "SELECT e.device_id,e.capability_id,e.announced_at_ms
                 FROM orbit_capability_executors e
                 JOIN devices d ON d.id=e.device_id AND d.revoked=0
                 WHERE e.capability_id=?1
                 ORDER BY e.device_id",
            )
            .map_err(repo_error)?;
        let rows = statement
            .query_map([capability_id], capability_executor_from_row)
            .map_err(repo_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }

    /// Every capability announced by at least one active device.
    pub fn announced_capability_ids(&self) -> Result<BTreeSet<String>, RepositoryError> {
        Ok(self
            .capability_executor_announcements()?
            .into_iter()
            .map(|announcement| announcement.capability_id)
            .collect())
    }

    /// Whether this exact active device announced this exact capability.
    pub fn capability_announced(
        &self,
        device_id: &str,
        capability_id: &str,
    ) -> Result<bool, RepositoryError> {
        self.query_row(
            "SELECT EXISTS(SELECT 1 FROM orbit_capability_executors e
                 JOIN devices d ON d.id=e.device_id AND d.revoked=0
                 WHERE e.device_id=?1 AND e.capability_id=?2)",
            params![device_id, capability_id],
            |row| row.get(0),
        )
        .map_err(repo_error)
    }

    /// Record a trusted composition's capability contract. Idempotent for an
    /// identical row; a different provider or action contract conflicts.
    pub fn install_capability(
        &mut self,
        installation: &CapabilityInstallation,
    ) -> Result<CapabilityInstallation, RepositoryError> {
        if !valid_ident(&installation.capability_id)
            || !valid_ident(&installation.provider)
            || !PERMISSIONS.contains(&installation.permission.as_str())
            || installation.installed_at_ms < 0
            || !json_array(&installation.actions_json)
            || !json_string_array(&installation.requires_json)
        {
            return Err(RepositoryError::Conflict);
        }
        if let Some(existing) = self.capability_installation(&installation.capability_id)? {
            // A published contract is immutable: changing it requires an
            // explicit uninstall/install cycle rather than a silent upgrade.
            // The install timestamp is metadata, not part of the contract, so a
            // replay at a different time stays idempotent.
            return if existing.capability_id == installation.capability_id
                && existing.provider == installation.provider
                && existing.permission == installation.permission
                && existing.actions_json == installation.actions_json
                && existing.requires_json == installation.requires_json
            {
                Ok(existing)
            } else {
                Err(RepositoryError::Conflict)
            };
        }
        self.execute(
            "INSERT INTO orbit_capability_installations
             (capability_id,provider,permission,actions_json,requires_json,installed_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                installation.capability_id,
                installation.provider,
                installation.permission,
                installation.actions_json,
                installation.requires_json,
                installation.installed_at_ms
            ],
        )
        .map_err(repo_error)?;
        self.capability_installation(&installation.capability_id)?
            .ok_or(RepositoryError::Conflict)
    }

    /// Remove an installation and all of its project grants atomically.
    pub fn uninstall_capability(&mut self, capability_id: &str) -> Result<bool, RepositoryError> {
        self.execute_batch("SAVEPOINT orbit_capability_uninstall")
            .map_err(repo_error)?;
        let outcome = (|| -> Result<bool, RepositoryError> {
            self.execute(
                "DELETE FROM orbit_capability_grants WHERE capability_id=?1",
                [capability_id],
            )
            .map_err(repo_error)?;
            let affected = self
                .execute(
                    "DELETE FROM orbit_capability_installations WHERE capability_id=?1",
                    [capability_id],
                )
                .map_err(repo_error)?;
            Ok(affected > 0)
        })();
        match outcome {
            Ok(removed) => {
                self.execute_batch("RELEASE SAVEPOINT orbit_capability_uninstall")
                    .map_err(repo_error)?;
                Ok(removed)
            }
            Err(error) => {
                let _ = self.execute_batch(
                    "ROLLBACK TO SAVEPOINT orbit_capability_uninstall; RELEASE SAVEPOINT orbit_capability_uninstall",
                );
                Err(error)
            }
        }
    }

    pub fn capability_installations(&self) -> Result<Vec<CapabilityInstallation>, RepositoryError> {
        let mut statement = self
            .prepare(
                "SELECT capability_id,provider,permission,actions_json,requires_json,installed_at_ms
                 FROM orbit_capability_installations ORDER BY capability_id",
            )
            .map_err(repo_error)?;
        let rows = statement
            .query_map([], capability_installation_from_row)
            .map_err(repo_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }

    pub fn capability_installation(
        &self,
        id: &str,
    ) -> Result<Option<CapabilityInstallation>, RepositoryError> {
        self.query_row(
            "SELECT capability_id,provider,permission,actions_json,requires_json,installed_at_ms
             FROM orbit_capability_installations WHERE capability_id=?1",
            [id],
            capability_installation_from_row,
        )
        .optional()
        .map_err(repo_error)
    }

    /// Grants require both an installation and an existing project. Re-granting
    /// the same (capability, project) pair returns the original grant.
    pub fn grant_capability(
        &mut self,
        capability_id: &str,
        project_id: &str,
        granted_by: &str,
        at_ms: i64,
    ) -> Result<CapabilityGrant, RepositoryError> {
        if !valid_ident(capability_id)
            || project_id.trim().is_empty()
            || granted_by.trim().is_empty()
            || at_ms < 0
        {
            return Err(RepositoryError::Conflict);
        }
        if self.capability_installation(capability_id)?.is_none() {
            return Err(RepositoryError::Conflict);
        }
        let project_exists: bool = self
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                [project_id],
                |row| row.get(0),
            )
            .map_err(repo_error)?;
        if !project_exists {
            return Err(RepositoryError::Conflict);
        }
        self.execute(
            "INSERT OR IGNORE INTO orbit_capability_grants
             (capability_id,project_id,granted_by,granted_at_ms) VALUES (?1,?2,?3,?4)",
            params![capability_id, project_id, granted_by, at_ms],
        )
        .map_err(repo_error)?;
        self.query_row(
            "SELECT capability_id,project_id,granted_by,granted_at_ms
             FROM orbit_capability_grants WHERE capability_id=?1 AND project_id=?2",
            params![capability_id, project_id],
            |row| {
                Ok(CapabilityGrant {
                    capability_id: row.get(0)?,
                    project_id: row.get(1)?,
                    granted_by: row.get(2)?,
                    granted_at_ms: row.get(3)?,
                })
            },
        )
        .map_err(repo_error)
    }

    pub fn revoke_capability(
        &mut self,
        capability_id: &str,
        project_id: &str,
    ) -> Result<bool, RepositoryError> {
        let affected = self
            .execute(
                "DELETE FROM orbit_capability_grants WHERE capability_id=?1 AND project_id=?2",
                params![capability_id, project_id],
            )
            .map_err(repo_error)?;
        Ok(affected > 0)
    }

    pub fn capability_grants(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<CapabilityGrant>, RepositoryError> {
        let mut statement = self
            .prepare(
                "SELECT capability_id,project_id,granted_by,granted_at_ms
                 FROM orbit_capability_grants WHERE (?1 IS NULL OR project_id=?1)
                 ORDER BY project_id, capability_id",
            )
            .map_err(repo_error)?;
        let rows = statement
            .query_map([project_id], |row| {
                Ok(CapabilityGrant {
                    capability_id: row.get(0)?,
                    project_id: row.get(1)?,
                    granted_by: row.get(2)?,
                    granted_at_ms: row.get(3)?,
                })
            })
            .map_err(repo_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }

    /// The project-scoped authorization set used by capability execution paths.
    pub fn authorized_capabilities(
        &self,
        project_id: &str,
    ) -> Result<BTreeSet<String>, RepositoryError> {
        Ok(self
            .capability_grants(Some(project_id))?
            .into_iter()
            .map(|grant| grant.capability_id)
            .collect())
    }

    /// Receipts are immutable and keyed by the action command ID. Replaying an
    /// identical receipt returns the stored row; different values conflict.
    pub fn record_capability_receipt(
        &mut self,
        receipt: &CapabilityReceipt,
    ) -> Result<CapabilityReceipt, RepositoryError> {
        if !valid_command_id(&receipt.command_id)
            || receipt.item_id.trim().is_empty()
            || !valid_ident(&receipt.capability)
            || receipt.action_id.trim().is_empty()
            || !valid_ident(&receipt.provider)
            || !OUTCOMES.contains(&receipt.outcome.as_str())
            || !receipt.detail.is_object()
            || receipt.recorded_at_ms < 0
        {
            return Err(RepositoryError::Conflict);
        }
        let detail = serde_json::to_string(&receipt.detail)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        self.execute(
            "INSERT OR IGNORE INTO orbit_blueprint_action_receipts
             (command_id,item_id,capability,action_id,provider,outcome,detail_json,recorded_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                receipt.command_id,
                receipt.item_id,
                receipt.capability,
                receipt.action_id,
                receipt.provider,
                receipt.outcome,
                detail,
                receipt.recorded_at_ms
            ],
        )
        .map_err(repo_error)?;
        let stored = self
            .capability_receipt(&receipt.command_id)?
            .ok_or(RepositoryError::Conflict)?;
        if stored == *receipt {
            Ok(stored)
        } else {
            Err(RepositoryError::Conflict)
        }
    }

    /// Explicitly resolve a previously `uncertain` receipt. Never dispatches and
    /// never re-claims; only the trusted runtime calls this after a human (or
    /// policy) decision.
    pub fn reconcile_capability_receipt(
        &mut self,
        command_id: &str,
        outcome: &str,
        detail: &Value,
    ) -> Result<CapabilityReceipt, RepositoryError> {
        if !matches!(outcome, "succeeded" | "failed") || !detail.is_object() {
            return Err(RepositoryError::Conflict);
        }
        let serialized = serde_json::to_string(detail)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        let changed = self
            .execute(
                "UPDATE orbit_blueprint_action_receipts SET outcome=?2, detail_json=?3
                 WHERE command_id=?1 AND outcome='uncertain'",
                params![command_id, outcome, serialized],
            )
            .map_err(repo_error)?;
        if changed == 0 {
            let existing = self
                .capability_receipt(command_id)?
                .ok_or(RepositoryError::Conflict)?;
            return if existing.outcome == outcome && existing.detail == *detail {
                Ok(existing)
            } else {
                Err(RepositoryError::Conflict)
            };
        }
        self.capability_receipt(command_id)?
            .ok_or(RepositoryError::Conflict)
    }

    /// Explicitly settle an action whose durable outcome is `claimed` (the
    /// process died between claim and result) or `uncertain`, and also replay an
    /// already settled identical decision.
    ///
    /// This never dispatches a capability and never claims a request. A
    /// `pending` request has not been executed, so it must go through
    /// `claim`/`execute` instead. The request state transition and the receipt
    /// write commit in one savepoint.
    pub fn settle_blueprint_action(
        &mut self,
        command_id: &str,
        outcome: &str,
        provider: &str,
        detail: &Value,
        recorded_at_ms: i64,
    ) -> Result<CapabilityReceipt, RepositoryError> {
        if !matches!(outcome, "succeeded" | "failed")
            || !detail.is_object()
            || !valid_command_id(command_id)
            || !valid_ident(provider)
            || recorded_at_ms < 0
        {
            return Err(RepositoryError::Conflict);
        }
        let request = self
            .blueprint_action_request(command_id)?
            .ok_or(RepositoryError::Conflict)?;
        // `pending` is an unexecuted intent; settling it would invent an effect.
        if request.state == "pending" {
            return Err(RepositoryError::Conflict);
        }
        let existing = self.capability_receipt(command_id)?;
        if let Some(receipt) = &existing {
            if receipt.item_id != request.item_id
                || receipt.capability != request.capability
                || receipt.action_id != request.action_id
                || receipt.provider != provider
            {
                return Err(RepositoryError::Conflict);
            }
            // A settled outcome is immutable: only the recorded decision replays.
            if receipt.outcome != "uncertain" && receipt.outcome != outcome {
                return Err(RepositoryError::Conflict);
            }
            if receipt.outcome == outcome && receipt.detail != *detail {
                return Err(RepositoryError::Conflict);
            }
        }
        if request.state == outcome {
            let receipt = existing.ok_or(RepositoryError::Conflict)?;
            return if receipt.outcome == outcome && receipt.detail == *detail {
                Ok(receipt)
            } else {
                Err(RepositoryError::Conflict)
            };
        }
        if request.state != "claimed" && request.state != "uncertain" {
            return Err(RepositoryError::Conflict);
        }
        // The local authority path records its effect through the provider
        // itself, so it never supplies an effect document here.
        self.write_action_settlement(
            &request,
            Settlement {
                outcome,
                provider,
                detail,
                effect: None,
                recorded_at_ms,
                existing,
            },
        )
    }

    /// Record the outcome a connected executor reported for a claim it holds.
    ///
    /// This is the same single-savepoint write as [`Self::settle_blueprint_action`],
    /// with two deliberate differences: an executor may report `uncertain`, and
    /// the only state it may settle is its own `claimed` intent. An explicit
    /// reconcile stays the sole way *out* of `uncertain`, so an ambiguous
    /// external effect is never retried or silently confirmed by the reporter.
    ///
    /// `effect` is the capability's declared, validated durable effect. When it
    /// is present it is written **inside the same savepoint** as the receipt and
    /// the state transition, so the authority can never record one without the
    /// other.
    pub fn report_blueprint_action_result(
        &mut self,
        command_id: &str,
        outcome: &str,
        provider: &str,
        detail: &Value,
        effect: Option<&CapabilityEffectRecord<'_>>,
        recorded_at_ms: i64,
    ) -> Result<CapabilityReceipt, RepositoryError> {
        if !matches!(outcome, "succeeded" | "failed" | "uncertain")
            || !detail.is_object()
            || !valid_command_id(command_id)
            || !valid_ident(provider)
            || recorded_at_ms < 0
        {
            return Err(RepositoryError::Conflict);
        }
        let request = self
            .blueprint_action_request(command_id)?
            .ok_or(RepositoryError::Conflict)?;
        if request.state == "pending" {
            return Err(RepositoryError::Conflict);
        }
        // A described effect must still target this request's own item. The
        // runtime already refused a mismatch; this is the writer's own fence.
        if let Some(effect) = effect
            && (!valid_ident(effect.kind) || effect.item_id != request.item_id)
        {
            return Err(RepositoryError::Conflict);
        }
        let existing = self.capability_receipt(command_id)?;
        if let Some(receipt) = &existing {
            if receipt.item_id != request.item_id
                || receipt.capability != request.capability
                || receipt.action_id != request.action_id
                || receipt.provider != provider
            {
                return Err(RepositoryError::Conflict);
            }
            // Replaying the same outcome returns the original receipt, including
            // an `uncertain` one; the reporter never rewrites a settled outcome.
            if request.state == outcome {
                if receipt.outcome == outcome {
                    if let Some(effect) = effect {
                        // The detail carries the effect statement, so an identical
                        // statement replaying is a no-op; a different one cannot
                        // be applied over a settled row.
                        let stored = self
                            .reference_work_item_note(command_id)?
                            .ok_or(RepositoryError::Conflict)?;
                        if stored.item_id != effect.item_id
                            || stored.note != effect.note
                            || stored.tag.as_deref() != effect.tag
                        {
                            return Err(RepositoryError::Conflict);
                        }
                    }
                    return Ok(receipt.clone());
                }
                return Err(RepositoryError::Conflict);
            }
            if receipt.outcome != "uncertain" {
                return Err(RepositoryError::Conflict);
            }
        } else if request.state == outcome {
            return Err(RepositoryError::Conflict);
        }
        // Only a live claim may be settled by the executor path. A different
        // outcome for an already resolved intent is not a correction.
        if request.state != "claimed" {
            return Err(RepositoryError::Conflict);
        }
        self.write_action_settlement(
            &request,
            Settlement {
                outcome,
                provider,
                detail,
                effect,
                recorded_at_ms,
                existing,
            },
        )
    }

    /// Write the receipt, the request state and (when declared) the durable
    /// effect in one savepoint. A settled request without its receipt, or an
    /// effect without the receipt that states it, is exactly what this prevents.
    fn write_action_settlement(
        &mut self,
        request: &BlueprintActionRequest,
        settlement: Settlement<'_>,
    ) -> Result<CapabilityReceipt, RepositoryError> {
        let Settlement {
            outcome,
            provider,
            detail,
            effect,
            recorded_at_ms,
            existing,
        } = settlement;
        let command_id = request.command_id.as_str();
        let serialized = serde_json::to_string(detail)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        self.execute_batch("SAVEPOINT orbit_blueprint_action_settle")
            .map_err(repo_error)?;
        let result = (|| -> Result<CapabilityReceipt, RepositoryError> {
            match &existing {
                None => {
                    self.execute(
                        "INSERT INTO orbit_blueprint_action_receipts
                         (command_id,item_id,capability,action_id,provider,outcome,detail_json,recorded_at_ms)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                        params![
                            request.command_id,
                            request.item_id,
                            request.capability,
                            request.action_id,
                            provider,
                            outcome,
                            serialized,
                            recorded_at_ms
                        ],
                    )
                    .map_err(repo_error)?;
                }
                Some(receipt) if receipt.outcome == "uncertain" => {
                    let changed = self
                        .execute(
                            "UPDATE orbit_blueprint_action_receipts SET outcome=?2, detail_json=?3
                             WHERE command_id=?1 AND outcome='uncertain'",
                            params![command_id, outcome, serialized],
                        )
                        .map_err(repo_error)?;
                    if changed != 1 {
                        return Err(RepositoryError::Conflict);
                    }
                }
                Some(_) => {}
            }
            let changed = self
                .execute(
                    "UPDATE orbit_blueprint_action_requests SET state=?2
                     WHERE command_id=?1 AND state IN ('claimed','uncertain')",
                    params![command_id, outcome],
                )
                .map_err(repo_error)?;
            if changed != 1 {
                return Err(RepositoryError::Conflict);
            }
            // The declared effect lives in the same savepoint as the receipt and
            // the state: `command_id` is the effect table's primary key, so one
            // command id can never own two notes, and a differing payload for an
            // existing row is a conflict rather than an overwrite.
            if let Some(effect) = effect {
                self.write_settlement_effect(command_id, effect, recorded_at_ms)?;
            }
            self.capability_receipt(command_id)?
                .ok_or(RepositoryError::Conflict)
        })();
        match result {
            Ok(receipt) => {
                self.execute_batch("RELEASE SAVEPOINT orbit_blueprint_action_settle")
                    .map_err(repo_error)?;
                Ok(receipt)
            }
            Err(error) => {
                let _ = self.execute_batch(
                    "ROLLBACK TO SAVEPOINT orbit_blueprint_action_settle; RELEASE SAVEPOINT orbit_blueprint_action_settle",
                );
                Err(error)
            }
        }
    }

    pub fn capability_receipt(
        &self,
        command_id: &str,
    ) -> Result<Option<CapabilityReceipt>, RepositoryError> {
        self.query_row(
            "SELECT command_id,item_id,capability,action_id,provider,outcome,detail_json,recorded_at_ms
             FROM orbit_blueprint_action_receipts WHERE command_id=?1",
            [command_id],
            capability_receipt_from_row,
        )
        .optional()
        .map_err(repo_error)
    }

    pub fn capability_receipts(
        &self,
        item_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CapabilityReceipt>, RepositoryError> {
        let mut statement = self
            .prepare(
                "SELECT command_id,item_id,capability,action_id,provider,outcome,detail_json,recorded_at_ms
                 FROM orbit_blueprint_action_receipts WHERE (?1 IS NULL OR item_id=?1)
                 ORDER BY recorded_at_ms, command_id LIMIT ?2",
            )
            .map_err(repo_error)?;
        let rows = statement
            .query_map(
                params![item_id, clamp_limit(limit)],
                capability_receipt_from_row,
            )
            .map_err(repo_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }

    pub fn capability_action_requests(
        &self,
        item_id: Option<&str>,
        state: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BlueprintActionRequest>, RepositoryError> {
        let mut statement = self
            .prepare(
                "SELECT command_id,item_id,item_revision,action_id,capability,plan_json,state
                 FROM orbit_blueprint_action_requests
                 WHERE (?1 IS NULL OR item_id=?1) AND (?2 IS NULL OR state=?2)
                 ORDER BY item_id, item_revision, action_id, command_id LIMIT ?3",
            )
            .map_err(repo_error)?;
        let rows = statement
            .query_map(params![item_id, state, clamp_limit(limit)], |row| {
                Ok(BlueprintActionRequest {
                    command_id: row.get(0)?,
                    item_id: row.get(1)?,
                    item_revision: row.get(2)?,
                    action_id: row.get(3)?,
                    capability: row.get(4)?,
                    plan_json: row.get(5)?,
                    state: row.get(6)?,
                })
            })
            .map_err(repo_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }

    /// Write one declared effect inside the settlement savepoint.
    ///
    /// `orbit_reference_work_item_notes.command_id` is the primary key, so the
    /// schema itself makes a second note for one command id impossible; the
    /// `INSERT OR IGNORE` plus the read-back comparison turns a differing
    /// payload into a conflict instead of an overwrite. The caller has already
    /// validated the payload against the capability's compile-time declaration.
    fn write_settlement_effect(
        &mut self,
        command_id: &str,
        effect: &CapabilityEffectRecord<'_>,
        created_at_ms: i64,
    ) -> Result<ReferenceWorkItemNote, RepositoryError> {
        if effect.note.trim().is_empty() || effect.note.chars().count() > 2_000 {
            return Err(RepositoryError::Conflict);
        }
        if let Some(tag) = effect.tag
            && (tag.trim().is_empty() || tag.chars().count() > 256)
        {
            return Err(RepositoryError::Conflict);
        }
        self.execute(
            "INSERT OR IGNORE INTO orbit_reference_work_item_notes
             (command_id,item_id,note,tag,created_at_ms) VALUES (?1,?2,?3,?4,?5)",
            params![
                command_id,
                effect.item_id,
                effect.note,
                effect.tag,
                created_at_ms
            ],
        )
        .map_err(repo_error)?;
        let stored = self
            .reference_work_item_note(command_id)?
            .ok_or(RepositoryError::Conflict)?;
        if stored.item_id == effect.item_id
            && stored.note == effect.note
            && stored.tag.as_deref() == effect.tag
        {
            Ok(stored)
        } else {
            Err(RepositoryError::Conflict)
        }
    }

    /// Durable effect of the trusted reference capability. The command ID is the
    /// idempotency key: a replay returns the original row instead of appending a
    /// second note. Reusing the ID with different content conflicts.
    pub fn record_reference_work_item_note(
        &mut self,
        command_id: &str,
        item_id: &str,
        note: &str,
        tag: Option<&str>,
        created_at_ms: i64,
    ) -> Result<ReferenceWorkItemNote, RepositoryError> {
        if !valid_command_id(command_id)
            || item_id.trim().is_empty()
            || note.trim().is_empty()
            || note.chars().count() > 2_000
            || created_at_ms < 0
        {
            return Err(RepositoryError::Conflict);
        }
        if let Some(tag) = tag
            && (tag.trim().is_empty() || tag.chars().count() > 256)
        {
            return Err(RepositoryError::Conflict);
        }
        if self.blueprint_work_item(item_id)?.is_none() {
            return Err(RepositoryError::Conflict);
        }
        self.execute(
            "INSERT OR IGNORE INTO orbit_reference_work_item_notes
             (command_id,item_id,note,tag,created_at_ms) VALUES (?1,?2,?3,?4,?5)",
            params![command_id, item_id, note, tag, created_at_ms],
        )
        .map_err(repo_error)?;
        let stored = self
            .reference_work_item_note(command_id)?
            .ok_or(RepositoryError::Conflict)?;
        if stored.item_id == item_id && stored.note == note && stored.tag.as_deref() == tag {
            Ok(stored)
        } else {
            Err(RepositoryError::Conflict)
        }
    }

    pub fn reference_work_item_note(
        &self,
        command_id: &str,
    ) -> Result<Option<ReferenceWorkItemNote>, RepositoryError> {
        self.query_row(
            "SELECT command_id,item_id,note,tag,created_at_ms
             FROM orbit_reference_work_item_notes WHERE command_id=?1",
            [command_id],
            reference_note_from_row,
        )
        .optional()
        .map_err(repo_error)
    }

    pub fn reference_work_item_notes(
        &self,
        item_id: &str,
    ) -> Result<Vec<ReferenceWorkItemNote>, RepositoryError> {
        let mut statement = self
            .prepare(
                "SELECT command_id,item_id,note,tag,created_at_ms
                 FROM orbit_reference_work_item_notes WHERE item_id=?1
                 ORDER BY created_at_ms, command_id",
            )
            .map_err(repo_error)?;
        let rows = statement
            .query_map([item_id], reference_note_from_row)
            .map_err(repo_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }

    pub fn reference_work_item_note_count(&self, item_id: &str) -> Result<u64, RepositoryError> {
        let count: i64 = self
            .query_row(
                "SELECT COUNT(*) FROM orbit_reference_work_item_notes WHERE item_id=?1",
                [item_id],
                |row| row.get(0),
            )
            .map_err(repo_error)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

fn json_array(document: &str) -> bool {
    matches!(serde_json::from_str::<Value>(document), Ok(Value::Array(_)))
}

fn json_string_array(document: &str) -> bool {
    match serde_json::from_str::<Value>(document) {
        Ok(Value::Array(items)) => items.iter().all(Value::is_string),
        _ => false,
    }
}

fn capability_installation_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CapabilityInstallation> {
    Ok(CapabilityInstallation {
        capability_id: row.get(0)?,
        provider: row.get(1)?,
        permission: row.get(2)?,
        actions_json: row.get(3)?,
        requires_json: row.get(4)?,
        installed_at_ms: row.get(5)?,
    })
}

fn capability_receipt_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CapabilityReceipt> {
    let detail: String = row.get(6)?;
    let detail = serde_json::from_str(&detail).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(CapabilityReceipt {
        command_id: row.get(0)?,
        item_id: row.get(1)?,
        capability: row.get(2)?,
        action_id: row.get(3)?,
        provider: row.get(4)?,
        outcome: row.get(5)?,
        detail,
        recorded_at_ms: row.get(7)?,
    })
}

fn reference_note_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReferenceWorkItemNote> {
    Ok(ReferenceWorkItemNote {
        command_id: row.get(0)?,
        item_id: row.get(1)?,
        note: row.get(2)?,
        tag: row.get(3)?,
        created_at_ms: row.get(4)?,
    })
}

fn capability_executor_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CapabilityExecutorAnnouncement> {
    Ok(CapabilityExecutorAnnouncement {
        device_id: row.get(0)?,
        capability_id: row.get(1)?,
        announced_at_ms: row.get(2)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_application::ProjectRepository;
    use orbit_domain::{Project, ProjectId};
    use std::collections::BTreeMap;

    fn installation() -> CapabilityInstallation {
        CapabilityInstallation {
            capability_id: "orbit.reference.work-item-note".into(),
            provider: "orbit".into(),
            permission: "mutate".into(),
            actions_json: r#"[{"action_id":"append_note"}]"#.into(),
            requires_json: "[]".into(),
            installed_at_ms: 10,
        }
    }

    fn project(store: &mut SqliteStore) {
        let project =
            Project::new(ProjectId::new("project-1").unwrap(), "Reference").expect("project");
        ProjectRepository::save(store, project).expect("save project");
    }

    #[test]
    fn installation_is_idempotent_and_contract_changes_conflict() {
        let mut store = SqliteStore::in_memory().unwrap();
        let stored = store.install_capability(&installation()).unwrap();
        assert_eq!(stored, installation());
        assert_eq!(store.install_capability(&installation()).unwrap(), stored);
        // The install timestamp is metadata; replaying later is still idempotent.
        let mut later = installation();
        later.installed_at_ms = 99_999;
        assert_eq!(store.install_capability(&later).unwrap(), stored);
        let mut changed = installation();
        changed.provider = "elsewhere".into();
        assert_eq!(
            store.install_capability(&changed),
            Err(RepositoryError::Conflict)
        );
        let mut changed = installation();
        changed.actions_json = "[]".into();
        assert_eq!(
            store.install_capability(&changed),
            Err(RepositoryError::Conflict)
        );
        let mut invalid = installation();
        invalid.permission = "root".into();
        assert_eq!(
            store.install_capability(&invalid),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(store.capability_installations().unwrap().len(), 1);
    }

    #[test]
    fn uninstall_removes_installation_and_grants() {
        let mut store = SqliteStore::in_memory().unwrap();
        project(&mut store);
        store.install_capability(&installation()).unwrap();
        store
            .grant_capability("orbit.reference.work-item-note", "project-1", "tester", 20)
            .unwrap();
        assert!(
            store
                .uninstall_capability("orbit.reference.work-item-note")
                .unwrap()
        );
        assert!(
            store
                .capability_installation("orbit.reference.work-item-note")
                .unwrap()
                .is_none()
        );
        assert!(store.capability_grants(None).unwrap().is_empty());
        assert!(
            !store
                .uninstall_capability("orbit.reference.work-item-note")
                .unwrap()
        );
    }

    #[test]
    fn grant_requires_installation_and_existing_project() {
        let mut store = SqliteStore::in_memory().unwrap();
        project(&mut store);
        assert_eq!(
            store.grant_capability("orbit.reference.work-item-note", "project-1", "tester", 1),
            Err(RepositoryError::Conflict)
        );
        store.install_capability(&installation()).unwrap();
        assert_eq!(
            store.grant_capability("orbit.reference.work-item-note", "missing", "tester", 1),
            Err(RepositoryError::Conflict)
        );
        let grant = store
            .grant_capability("orbit.reference.work-item-note", "project-1", "tester", 1)
            .unwrap();
        assert_eq!(grant.granted_by, "tester");
        // Re-granting the same pair returns the original row.
        let replay = store
            .grant_capability("orbit.reference.work-item-note", "project-1", "other", 99)
            .unwrap();
        assert_eq!(replay, grant);
        assert_eq!(
            store.authorized_capabilities("project-1").unwrap(),
            BTreeSet::from(["orbit.reference.work-item-note".to_owned()])
        );
        assert!(
            store
                .authorized_capabilities("project-2")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn revoke_is_scoped_and_idempotent() {
        let mut store = SqliteStore::in_memory().unwrap();
        project(&mut store);
        store.install_capability(&installation()).unwrap();
        store
            .grant_capability("orbit.reference.work-item-note", "project-1", "tester", 1)
            .unwrap();
        assert!(
            store
                .revoke_capability("orbit.reference.work-item-note", "project-1")
                .unwrap()
        );
        assert!(
            !store
                .revoke_capability("orbit.reference.work-item-note", "project-1")
                .unwrap()
        );
        assert!(
            store
                .authorized_capabilities("project-1")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn receipts_are_idempotent_and_conflict_on_change() {
        let mut store = SqliteStore::in_memory().unwrap();
        let receipt = CapabilityReceipt {
            command_id: "action-1".into(),
            item_id: "item-1".into(),
            capability: "orbit.reference.work-item-note".into(),
            action_id: "append_note".into(),
            provider: "orbit".into(),
            outcome: "succeeded".into(),
            detail: serde_json::json!({"note_recorded": true}),
            recorded_at_ms: 5,
        };
        assert_eq!(store.record_capability_receipt(&receipt).unwrap(), receipt);
        assert_eq!(store.record_capability_receipt(&receipt).unwrap(), receipt);
        let mut changed = receipt.clone();
        changed.outcome = "failed".into();
        assert_eq!(
            store.record_capability_receipt(&changed),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(store.capability_receipts(None, 10).unwrap().len(), 1);
        assert_eq!(
            store.capability_receipts(Some("item-1"), 10).unwrap().len(),
            1
        );
        assert!(
            store
                .capability_receipts(Some("item-2"), 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn uncertain_receipt_can_only_be_reconciled_once() {
        let mut store = SqliteStore::in_memory().unwrap();
        let receipt = CapabilityReceipt {
            command_id: "action-uncertain".into(),
            item_id: "item-1".into(),
            capability: "orbit.reference.work-item-note".into(),
            action_id: "append_note".into(),
            provider: "orbit".into(),
            outcome: "uncertain".into(),
            detail: serde_json::json!({}),
            recorded_at_ms: 5,
        };
        store.record_capability_receipt(&receipt).unwrap();
        let detail = serde_json::json!({"reconciled_by":"tester","reconciled":true});
        let reconciled = store
            .reconcile_capability_receipt("action-uncertain", "succeeded", &detail)
            .unwrap();
        assert_eq!(reconciled.outcome, "succeeded");
        assert_eq!(reconciled.detail, detail);
        // Replaying the same decision is idempotent; a different one conflicts.
        assert_eq!(
            store
                .reconcile_capability_receipt("action-uncertain", "succeeded", &detail)
                .unwrap(),
            reconciled
        );
        assert_eq!(
            store.reconcile_capability_receipt("action-uncertain", "failed", &detail),
            Err(RepositoryError::Conflict)
        );
    }

    #[test]
    fn reference_notes_are_keyed_by_command_id() {
        let mut store = SqliteStore::in_memory().unwrap();
        project(&mut store);
        let schema = serde_json::json!({
            "id": "reference", "version": 1, "name": "Reference",
            "required_capabilities": [],
            "work_items": [{
                "id": "note-task", "name": "Note task", "initial_state": "open",
                "states": ["open", "closed"], "transitions": [{"from": "open", "to": "closed"}]
            }]
        });
        let blueprint: orbit_blueprint::Blueprint =
            serde_json::from_value(schema).expect("blueprint");
        store
            .publish_blueprint(&blueprint, &BTreeSet::new())
            .unwrap();
        store
            .bind_existing_project_blueprint("project-1", "reference", 1)
            .unwrap();
        store
            .create_blueprint_work_item("item-1", "project-1", "note-task", BTreeMap::new())
            .unwrap();
        let first = store
            .record_reference_work_item_note("action-1", "item-1", "hello", Some("demo"), 7)
            .unwrap();
        let replay = store
            .record_reference_work_item_note("action-1", "item-1", "hello", Some("demo"), 7)
            .unwrap();
        assert_eq!(first, replay);
        assert_eq!(store.reference_work_item_note_count("item-1").unwrap(), 1);
        assert_eq!(
            store.record_reference_work_item_note("action-1", "item-1", "other", None, 7),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(
            store.record_reference_work_item_note("action-2", "missing", "hello", None, 7),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(
            store.record_reference_work_item_note("action-2", "item-1", "", None, 7),
            Err(RepositoryError::Conflict)
        );
        let long = "x".repeat(2_001);
        assert_eq!(
            store.record_reference_work_item_note("action-2", "item-1", &long, None, 7),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(store.reference_work_item_note_count("item-1").unwrap(), 1);
    }
}

#[cfg(test)]
mod settlement_tests {
    use super::*;
    use orbit_application::ProjectRepository;
    use orbit_domain::{Project, ProjectId};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn store_with_items() -> SqliteStore {
        let mut store = SqliteStore::in_memory().unwrap();
        let project =
            Project::new(ProjectId::new("project-1").unwrap(), "Reference").expect("project");
        ProjectRepository::save(&mut store, project).expect("save project");
        let blueprint: orbit_blueprint::Blueprint = serde_json::from_value(json!({
            "id": "reference", "version": 1, "name": "Reference",
            "required_capabilities": [],
            "work_items": [{
                "id": "note-task", "name": "Note task", "initial_state": "open",
                "states": ["open", "closed"], "transitions": [{"from": "open", "to": "closed"}]
            }]
        }))
        .expect("blueprint");
        store
            .publish_blueprint(&blueprint, &BTreeSet::new())
            .unwrap();
        store
            .bind_existing_project_blueprint("project-1", "reference", 1)
            .unwrap();
        for item in ["item-1", "item-2"] {
            store
                .create_blueprint_work_item(item, "project-1", "note-task", BTreeMap::new())
                .unwrap();
        }
        store
    }

    fn request_row(store: &mut SqliteStore, command_id: &str, revision: u64) {
        // Distinct revisions keep the unique(item, revision, action) fence happy;
        // settlement only depends on the request's state and identity.
        store
            .execute(
                "INSERT INTO orbit_blueprint_action_requests
                 (command_id,item_id,item_revision,action_id,capability,plan_json)
                 VALUES (?1,'item-1',?2,'append_note','orbit.reference.work-item-note','{}')",
                params![command_id, revision],
            )
            .unwrap();
    }

    fn detail(actor: &str, recovered_from: &str) -> Value {
        json!({"reconciled": true, "reconciled_by": actor, "recovered_from": recovered_from})
    }

    #[test]
    fn claimed_without_receipt_is_settled_atomically() {
        let mut store = store_with_items();
        request_row(&mut store, "action-claimed", 0);
        store.claim_blueprint_action("action-claimed").unwrap();
        let receipt = store
            .settle_blueprint_action(
                "action-claimed",
                "failed",
                "orbit",
                &detail("operator", "claimed"),
                50,
            )
            .unwrap();
        assert_eq!(receipt.outcome, "failed");
        assert_eq!(receipt.provider, "orbit");
        assert_eq!(receipt.recorded_at_ms, 50);
        assert_eq!(receipt.item_id, "item-1");
        assert_eq!(receipt.detail, detail("operator", "claimed"));
        assert_eq!(
            store
                .blueprint_action_request("action-claimed")
                .unwrap()
                .unwrap()
                .state,
            "failed"
        );
        // Identical decision replays; a different actor or decision conflicts.
        assert_eq!(
            store
                .settle_blueprint_action(
                    "action-claimed",
                    "failed",
                    "orbit",
                    &detail("operator", "claimed"),
                    99,
                )
                .unwrap(),
            receipt
        );
        assert_eq!(
            store.settle_blueprint_action(
                "action-claimed",
                "failed",
                "orbit",
                &detail("someone-else", "claimed"),
                99,
            ),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(
            store.settle_blueprint_action(
                "action-claimed",
                "succeeded",
                "orbit",
                &detail("operator", "claimed"),
                99,
            ),
            Err(RepositoryError::Conflict)
        );
    }

    #[test]
    fn claimed_with_uncertain_receipt_is_settled_and_keeps_its_record_time() {
        let mut store = store_with_items();
        request_row(&mut store, "action-uncertain", 0);
        store.claim_blueprint_action("action-uncertain").unwrap();
        store
            .record_capability_receipt(&CapabilityReceipt {
                command_id: "action-uncertain".into(),
                item_id: "item-1".into(),
                capability: "orbit.reference.work-item-note".into(),
                action_id: "append_note".into(),
                provider: "orbit".into(),
                outcome: "uncertain".into(),
                detail: json!({"state": "uncertain"}),
                recorded_at_ms: 20,
            })
            .unwrap();
        let receipt = store
            .settle_blueprint_action(
                "action-uncertain",
                "succeeded",
                "orbit",
                &detail("operator", "claimed"),
                60,
            )
            .unwrap();
        assert_eq!(receipt.outcome, "succeeded");
        assert_eq!(receipt.detail, detail("operator", "claimed"));
        // The original execution time is preserved; only the decision changes.
        assert_eq!(receipt.recorded_at_ms, 20);
        assert_eq!(
            store
                .blueprint_action_request("action-uncertain")
                .unwrap()
                .unwrap()
                .state,
            "succeeded"
        );
    }

    #[test]
    fn an_executor_result_settles_claimed_to_uncertain_atomically() {
        let mut store = store_with_items();
        request_row(&mut store, "action-executor", 0);
        store.claim_blueprint_action("action-executor").unwrap();
        let reported = json!({"state": "uncertain", "detail": "lost", "reported_by": "exec-1"});
        let receipt = store
            .report_blueprint_action_result(
                "action-executor",
                "uncertain",
                "orbit",
                &reported,
                None,
                10,
            )
            .unwrap();
        assert_eq!(receipt.outcome, "uncertain");
        assert_eq!(receipt.detail, reported);
        assert_eq!(
            store
                .blueprint_action_request("action-executor")
                .unwrap()
                .unwrap()
                .state,
            "uncertain",
            "the state and the receipt commit together"
        );
        // Replaying the same outcome returns the original receipt; a different
        // outcome on a settled intent is refused.
        assert_eq!(
            store
                .report_blueprint_action_result(
                    "action-executor",
                    "uncertain",
                    "orbit",
                    &reported,
                    None,
                    99
                )
                .unwrap(),
            receipt
        );
        assert_eq!(
            store.report_blueprint_action_result(
                "action-executor",
                "succeeded",
                "orbit",
                &reported,
                None,
                99
            ),
            Err(RepositoryError::Conflict)
        );
        // A pending intent never had an executor outcome to report.
        request_row(&mut store, "action-pending-executor", 1);
        assert_eq!(
            store.report_blueprint_action_result(
                "action-pending-executor",
                "failed",
                "orbit",
                &reported,
                None,
                10
            ),
            Err(RepositoryError::Conflict)
        );
        // An `uncertain` receipt is still only resolvable by an explicit
        // reconcile; the executor path cannot confirm it.
        assert!(
            store
                .report_blueprint_action_result(
                    "action-executor",
                    "succeeded",
                    "orbit",
                    &reported,
                    None,
                    99
                )
                .is_err()
        );
        let reconciled = store
            .settle_blueprint_action(
                "action-executor",
                "succeeded",
                "orbit",
                &detail("operator", "uncertain"),
                100,
            )
            .unwrap();
        assert_eq!(reconciled.outcome, "succeeded");
    }

    #[test]
    fn pending_settled_and_disagreeing_requests_are_refused() {
        let mut store = store_with_items();
        // Pending: never executed, so it cannot be settled.
        request_row(&mut store, "action-pending", 0);
        assert_eq!(
            store.settle_blueprint_action(
                "action-pending",
                "failed",
                "orbit",
                &detail("operator", "claimed"),
                10,
            ),
            Err(RepositoryError::Conflict)
        );
        assert!(
            store
                .capability_receipt("action-pending")
                .unwrap()
                .is_none()
        );

        // Settled without a receipt: the authority refuses to invent one.
        request_row(&mut store, "action-settled", 1);
        store.claim_blueprint_action("action-settled").unwrap();
        store
            .record_blueprint_action_result("action-settled", "succeeded")
            .unwrap();
        assert_eq!(
            store.settle_blueprint_action(
                "action-settled",
                "failed",
                "orbit",
                &detail("operator", "claimed"),
                10,
            ),
            Err(RepositoryError::Conflict)
        );

        // Claimed with a settled receipt that disagrees with the decision.
        request_row(&mut store, "action-disagree", 2);
        store.claim_blueprint_action("action-disagree").unwrap();
        store
            .record_capability_receipt(&CapabilityReceipt {
                command_id: "action-disagree".into(),
                item_id: "item-1".into(),
                capability: "orbit.reference.work-item-note".into(),
                action_id: "append_note".into(),
                provider: "orbit".into(),
                outcome: "succeeded".into(),
                detail: json!({}),
                recorded_at_ms: 5,
            })
            .unwrap();
        assert_eq!(
            store.settle_blueprint_action(
                "action-disagree",
                "failed",
                "orbit",
                &detail("operator", "claimed"),
                10,
            ),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(
            store
                .capability_receipt("action-disagree")
                .unwrap()
                .unwrap()
                .outcome,
            "succeeded"
        );

        // Unknown command ids and invalid outcomes fail closed.
        assert_eq!(
            store.settle_blueprint_action(
                "missing",
                "failed",
                "orbit",
                &detail("operator", "claimed"),
                10,
            ),
            Err(RepositoryError::Conflict)
        );
        request_row(&mut store, "action-invalid", 3);
        store.claim_blueprint_action("action-invalid").unwrap();
        assert_eq!(
            store.settle_blueprint_action(
                "action-invalid",
                "uncertain",
                "orbit",
                &detail("operator", "claimed"),
                10,
            ),
            Err(RepositoryError::Conflict)
        );
    }

    fn effect<'a>(
        item_id: &'a str,
        note: &'a str,
        tag: Option<&'a str>,
    ) -> CapabilityEffectRecord<'a> {
        CapabilityEffectRecord {
            kind: "work_item_note",
            item_id,
            note,
            tag,
        }
    }

    #[test]
    fn a_declared_effect_is_written_in_the_settlement_savepoint_exactly_once() {
        let mut store = store_with_items();
        request_row(&mut store, "action-effect", 0);
        store.claim_blueprint_action("action-effect").unwrap();
        let reported = json!({"state": "succeeded", "effect": {"applied": true}});
        let receipt = store
            .report_blueprint_action_result(
                "action-effect",
                "succeeded",
                "orbit",
                &reported,
                Some(&effect("item-1", "hello", Some("demo"))),
                70,
            )
            .unwrap();
        assert_eq!(receipt.outcome, "succeeded");
        // Receipt, state and effect all exist because one savepoint committed.
        let stored = store
            .reference_work_item_note("action-effect")
            .unwrap()
            .expect("note");
        assert_eq!(stored.item_id, "item-1");
        assert_eq!(stored.note, "hello");
        assert_eq!(stored.tag.as_deref(), Some("demo"));
        assert_eq!(stored.created_at_ms, 70);
        assert_eq!(store.reference_work_item_note_count("item-1").unwrap(), 1);

        // The identical report replays; a different payload for the same command
        // id is a conflict, never an overwrite and never a second row.
        assert_eq!(
            store
                .report_blueprint_action_result(
                    "action-effect",
                    "succeeded",
                    "orbit",
                    &reported,
                    Some(&effect("item-1", "hello", Some("demo"))),
                    99,
                )
                .unwrap(),
            receipt
        );
        assert_eq!(
            store.report_blueprint_action_result(
                "action-effect",
                "succeeded",
                "orbit",
                &reported,
                Some(&effect("item-1", "another", None)),
                99,
            ),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(store.reference_work_item_note_count("item-1").unwrap(), 1);
    }

    #[test]
    fn an_effect_for_another_item_is_refused_without_settling_anything() {
        let mut store = store_with_items();
        request_row(&mut store, "action-mismatch", 0);
        store.claim_blueprint_action("action-mismatch").unwrap();
        let reported = json!({"state": "succeeded"});
        assert_eq!(
            store.report_blueprint_action_result(
                "action-mismatch",
                "succeeded",
                "orbit",
                &reported,
                Some(&effect("item-2", "hello", None)),
                70,
            ),
            Err(RepositoryError::Conflict)
        );
        // Nothing partially applied: no note, no receipt, still claimed.
        assert_eq!(store.reference_work_item_note_count("item-2").unwrap(), 0);
        assert!(
            store
                .capability_receipt("action-mismatch")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .blueprint_action_request("action-mismatch")
                .unwrap()
                .unwrap()
                .state,
            "claimed"
        );
        // An empty or over-long payload is refused by the writer even before the
        // savepoint, so an unvalidated caller cannot smuggle one in.
        assert_eq!(
            store.report_blueprint_action_result(
                "action-mismatch",
                "succeeded",
                "orbit",
                &reported,
                Some(&effect("item-1", "  ", None)),
                70,
            ),
            Err(RepositoryError::Conflict)
        );
        let long = "x".repeat(2_001);
        assert_eq!(
            store.report_blueprint_action_result(
                "action-mismatch",
                "succeeded",
                "orbit",
                &reported,
                Some(&effect("item-1", &long, None)),
                70,
            ),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(store.reference_work_item_note_count("item-1").unwrap(), 0);
    }
}
