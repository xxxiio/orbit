//! Attachment commands for local-authoritative and connected deployments.
//!
//! Local mode uses the embedded `OrbitClient`; connected mode stages uploads
//! durably in the replica and only reports a server attachment as accepted
//! after the server returns a validated receipt. Pending uploads are always
//! reported separately and are never labelled as accepted.
use crate::{AttachmentSubcommand, CliError};
use orbit_client::deployment::Deployment;
use orbit_client::unified::{AuthorityConnection, ClientConnectionError};
use orbit_client::{AttachmentDto, OrbitClient, PendingAttachmentDto};
use serde_json::{Value, json};
use std::path::Path;

const LOCAL_FLUSH_MESSAGE: &str = "`orbit attachment flush` is only available in a connected deployment; \
     the local authority accepts attachments synchronously and has nothing to flush";

fn client_error(error: orbit_client::ClientError) -> CliError {
    CliError::Application(error.message)
}

fn connection_error(error: ClientConnectionError) -> CliError {
    match error {
        ClientConnectionError::Local(error) => client_error(error),
        ClientConnectionError::Unavailable(message) => CliError::Usage(message),
        other => CliError::Usage(format!("{other:?}")),
    }
}

/// Connected identity is optional so staged uploads can still be listed and
/// discarded offline. Server-backed operations report an honest offline error.
fn connected_identity() -> (Option<String>, Option<String>) {
    let token = std::env::var("ORBIT_SYNC_HTTP_TOKEN")
        .ok()
        .filter(|value| !value.is_empty());
    let device = std::env::var("ORBIT_AUTHORITY_DEVICE")
        .ok()
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control));
    (device, token)
}

fn attachment_json(attachment: &AttachmentDto) -> Value {
    json!({
        "id": attachment.id,
        "owner_kind": attachment.owner_kind,
        "owner_id": attachment.owner_id,
        "filename": attachment.filename,
        "content_type": attachment.content_type,
        "size_bytes": attachment.size_bytes,
        "checksum": attachment.checksum,
        "created_at_ms": attachment.created_at_ms,
        "description": attachment.description,
        "is_image": attachment.is_image,
    })
}

fn pending_json(pending: &PendingAttachmentDto) -> Value {
    json!({
        "attachment": attachment_json(&pending.attachment),
        "state": pending.state,
        "last_error": pending.last_error,
    })
}

fn human(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "invalid response".into())
}

pub fn run(
    deployment: &Deployment,
    db_path: &Path,
    command: AttachmentSubcommand,
    json_output: bool,
) -> Result<String, CliError> {
    let value = match deployment {
        Deployment::Local { .. } => run_local(db_path, command)?,
        Deployment::Connected { server, .. } => run_connected(server, db_path, command)?,
    };
    Ok(if json_output {
        human(&value)
    } else {
        human_text(&value)
    })
}

fn run_local(db_path: &Path, command: AttachmentSubcommand) -> Result<Value, CliError> {
    let mut client = OrbitClient::open(db_path).map_err(client_error)?;
    match command {
        AttachmentSubcommand::Add(args) => {
            let attachment = client
                .import_attachment(
                    args.owner_kind,
                    args.owner_id,
                    args.path.to_string_lossy().into_owned(),
                    args.description,
                )
                .map_err(client_error)?;
            Ok(json!({
                "accepted": true,
                "pending": false,
                "attachment": attachment_json(&attachment),
            }))
        }
        AttachmentSubcommand::List(args) => {
            let accepted = client
                .list_attachments(&args.owner_kind, &args.owner_id)
                .map_err(client_error)?;
            Ok(json!({
                "accepted": accepted.iter().map(attachment_json).collect::<Vec<_>>(),
                "staged": Vec::<Value>::new(),
            }))
        }
        AttachmentSubcommand::Inspect(args) => {
            let (attachment, content) =
                client.attachment_content(&args.id).map_err(client_error)?;
            Ok(json!({
                "pending": false,
                "verified_size_bytes": content.len(),
                "attachment": attachment_json(&attachment),
            }))
        }
        AttachmentSubcommand::Export(args) => {
            let destination = args.destination.to_string_lossy().into_owned();
            let attachment = client
                .export_attachment(&args.id, destination.clone())
                .map_err(client_error)?;
            Ok(json!({
                "pending": false,
                "destination": destination,
                "attachment": attachment_json(&attachment),
            }))
        }
        AttachmentSubcommand::Remove(args) => {
            let content_removed = client
                .remove_attachment(args.id.clone())
                .map_err(client_error)?;
            Ok(json!({
                "removed": true,
                "pending": false,
                "pending_discarded": false,
                "content_removed": content_removed,
                "attachment_id": args.id,
            }))
        }
        AttachmentSubcommand::Flush => Err(CliError::Usage(LOCAL_FLUSH_MESSAGE.into())),
    }
}

fn run_connected(
    server: &str,
    db_path: &Path,
    command: AttachmentSubcommand,
) -> Result<Value, CliError> {
    let (device, token) = connected_identity();
    let identity = match (device.as_deref(), token.as_deref()) {
        (Some(device), Some(token)) => Some((device, token)),
        _ => None,
    };
    let mut connection =
        AuthorityConnection::remote(db_path, server, identity).map_err(connection_error)?;
    match command {
        AttachmentSubcommand::Add(args) => {
            let attachment = connection
                .import_attachment(
                    args.owner_kind,
                    args.owner_id,
                    args.path.to_string_lossy().into_owned(),
                    args.description,
                )
                .map_err(connection_error)?;
            let pending = connection
                .pending_attachment_uploads()
                .map_err(connection_error)?
                .iter()
                .any(|item| item.attachment.id == attachment.id);
            Ok(json!({
                "accepted": !pending,
                "pending": pending,
                "attachment": attachment_json(&attachment),
            }))
        }
        AttachmentSubcommand::List(args) => {
            let accepted = connection
                .list_attachments(&args.owner_kind, &args.owner_id)
                .map_err(connection_error)?;
            let staged = connection
                .staged_attachments(&args.owner_kind, &args.owner_id)
                .map_err(connection_error)?;
            Ok(json!({
                "accepted": accepted.iter().map(attachment_json).collect::<Vec<_>>(),
                "staged": staged.iter().map(pending_json).collect::<Vec<_>>(),
            }))
        }
        AttachmentSubcommand::Inspect(args) => {
            let (attachment, content) = connection
                .attachment_content(&args.id)
                .map_err(connection_error)?;
            let pending = connection
                .pending_attachment_uploads()
                .map_err(connection_error)?
                .iter()
                .any(|item| item.attachment.id == args.id);
            Ok(json!({
                "pending": pending,
                "verified_size_bytes": content.len(),
                "attachment": attachment_json(&attachment),
            }))
        }
        AttachmentSubcommand::Export(args) => {
            let destination = args.destination.to_string_lossy().into_owned();
            let pending = connection
                .pending_attachment_uploads()
                .map_err(connection_error)?
                .iter()
                .any(|item| item.attachment.id == args.id);
            let attachment = connection
                .export_attachment(&args.id, destination.clone())
                .map_err(connection_error)?;
            Ok(json!({
                "pending": pending,
                "destination": destination,
                "attachment": attachment_json(&attachment),
            }))
        }
        AttachmentSubcommand::Remove(args) => {
            let pending_discarded = connection
                .pending_attachment_uploads()
                .map_err(connection_error)?
                .iter()
                .any(|item| item.attachment.id == args.id);
            let content_removed = connection
                .remove_attachment(args.id.clone())
                .map_err(connection_error)?;
            Ok(json!({
                "removed": true,
                "pending": pending_discarded,
                "pending_discarded": pending_discarded,
                "content_removed": content_removed,
                "attachment_id": args.id,
            }))
        }
        AttachmentSubcommand::Flush => {
            let outbox_before = connection
                .pending_command_count()
                .map_err(connection_error)?;
            let uploads_before = connection
                .pending_attachment_upload_count()
                .map_err(connection_error)?;
            let outcome = connection.flush().map_err(connection_error)?;
            let outbox_after = connection
                .pending_command_count()
                .map_err(connection_error)?;
            let uploads_after = connection
                .pending_attachment_upload_count()
                .map_err(connection_error)?;
            let outcome = match outcome {
                orbit_client::authority::FlushOutcome::Empty => {
                    json!({"status": "empty", "accepted_count": 0})
                }
                orbit_client::authority::FlushOutcome::Accepted(count) => {
                    json!({"status": "flushed", "accepted_count": count})
                }
                orbit_client::authority::FlushOutcome::Blocked {
                    accepted,
                    command_id,
                    reason,
                } => json!({
                    "status": "blocked",
                    "accepted_count": accepted,
                    "blocked_command": command_id,
                    "reason": reason,
                }),
            };
            Ok(json!({
                "pending_before": outbox_before,
                "uploads_pending_before": uploads_before,
                "pending_after": outbox_after,
                "uploads_pending_after": uploads_after,
                "outcome": outcome,
            }))
        }
    }
}

/// Human output intentionally leads with acceptance/pending language so a
/// staged upload is never read as an authoritative attachment.
fn human_text(value: &Value) -> String {
    if let Some(accepted) = value.get("accepted").and_then(Value::as_array) {
        let mut lines = vec![format!("Accepted attachments: {}", accepted.len())];
        for attachment in accepted {
            lines.push(format!(
                "  {}  {:<28}  {} bytes  {}",
                attachment["id"].as_str().unwrap_or_default(),
                attachment["filename"].as_str().unwrap_or_default(),
                attachment["size_bytes"].as_u64().unwrap_or_default(),
                attachment["checksum"].as_str().unwrap_or_default()
            ));
        }
        let staged = value
            .get("staged")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        lines.push(format!("Pending uploads: {}", staged.len()));
        for item in staged {
            let attachment = &item["attachment"];
            let detail = item["last_error"]
                .as_str()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default();
            lines.push(format!(
                "  [pending] {}  {:<28}  {}{detail}",
                attachment["id"].as_str().unwrap_or_default(),
                attachment["filename"].as_str().unwrap_or_default(),
                item["state"].as_str().unwrap_or_default()
            ));
        }
        return lines.join("\n");
    }
    if let Some(attachment) = value.get("attachment") {
        if let Some(pending) = value.get("pending").and_then(Value::as_bool) {
            let label = if pending { "[pending]" } else { "[accepted]" };
            let mut lines = vec![format!(
                "{label} {}",
                attachment["id"].as_str().unwrap_or_default()
            )];
            for field in [
                "owner_kind",
                "owner_id",
                "filename",
                "content_type",
                "size_bytes",
                "checksum",
                "created_at_ms",
                "description",
            ] {
                if let Some(field_value) = attachment.get(field)
                    && !field_value.is_null()
                {
                    lines.push(format!("  {field}: {field_value}"));
                }
            }
            if let Some(destination) = value.get("destination").and_then(Value::as_str) {
                lines.push(format!("  destination: {destination}"));
            }
            return lines.join("\n");
        }
        return format!(
            "attachment {}  {}\n  size: {} bytes\n  checksum: {}",
            attachment["id"].as_str().unwrap_or_default(),
            attachment["filename"].as_str().unwrap_or_default(),
            attachment["size_bytes"].as_u64().unwrap_or_default(),
            attachment["checksum"].as_str().unwrap_or_default()
        );
    }
    if value.get("pending_after").is_some() {
        let outcome = &value["outcome"];
        return format!(
            "Flush {}: accepted {}\n  pending outbox: {} -> {}\n  pending attachment uploads: {} -> {}",
            outcome["status"].as_str().unwrap_or("unknown"),
            outcome["accepted_count"].as_u64().unwrap_or_default(),
            value["pending_before"].as_i64().unwrap_or_default(),
            value["pending_after"].as_i64().unwrap_or_default(),
            value["uploads_pending_before"].as_i64().unwrap_or_default(),
            value["uploads_pending_after"].as_i64().unwrap_or_default(),
        );
    }
    if value.get("removed").is_some() {
        return format!(
            "Removed attachment {}{}",
            value["attachment_id"].as_str().unwrap_or_default(),
            if value["pending_discarded"].as_bool().unwrap_or(false) {
                " (pending staging row discarded locally)"
            } else {
                ""
            }
        );
    }
    human(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AttachmentAddArgs, AttachmentListArgs, Cli};
    use clap::Parser;

    #[test]
    fn attachment_command_family_parses() {
        let cli = Cli::try_parse_from([
            "orbit",
            "--json",
            "attachment",
            "add",
            "task",
            "task-1",
            "/tmp/file.txt",
            "--description",
            "note",
        ])
        .unwrap();
        let crate::Command::Attachment(command) = cli.command else {
            panic!("attachment command");
        };
        match command.command {
            AttachmentSubcommand::Add(args) => {
                assert_eq!(args.owner_kind, "task");
                assert_eq!(args.owner_id, "task-1");
                assert_eq!(args.description.as_deref(), Some("note"));
            }
            _ => panic!("add subcommand"),
        }

        let cli =
            Cli::try_parse_from(["orbit", "attachment", "list", "project", "project-1"]).unwrap();
        let crate::Command::Attachment(command) = cli.command else {
            panic!("attachment command");
        };
        assert!(matches!(command.command, AttachmentSubcommand::List(_)));

        let cli = Cli::try_parse_from(["orbit", "attachment", "flush"]).unwrap();
        let crate::Command::Attachment(command) = cli.command else {
            panic!("attachment command");
        };
        assert!(matches!(command.command, AttachmentSubcommand::Flush));
    }

    #[test]
    fn local_add_shape_is_always_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("notes.txt");
        std::fs::write(&source, b"hello attachment").unwrap();
        let db = dir.path().join("local.db");
        let mut client = OrbitClient::open(&db).unwrap();
        let project = client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "Attachment project".into(),
            })
            .unwrap();
        let value = run_local(
            &db,
            AttachmentSubcommand::Add(AttachmentAddArgs {
                owner_kind: "project".into(),
                owner_id: project.id.clone(),
                path: source.clone(),
                description: Some("fixture".into()),
            }),
        )
        .unwrap();
        assert_eq!(value["accepted"], true);
        assert_eq!(value["pending"], false);
        assert_eq!(value["attachment"]["filename"], "notes.txt");
        assert_eq!(value["attachment"]["owner_kind"], "project");
        assert_eq!(value["attachment"]["description"], "fixture");
        assert_eq!(value["attachment"]["size_bytes"], 16);
        assert_eq!(value["attachment"]["checksum"].as_str().unwrap().len(), 64);
    }

    #[test]
    fn local_list_separates_accepted_from_staged() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("list.txt");
        std::fs::write(&source, b"listed").unwrap();
        let db = dir.path().join("local.db");
        let mut client = OrbitClient::open(&db).unwrap();
        let project = client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "List project".into(),
            })
            .unwrap();
        run_local(
            &db,
            AttachmentSubcommand::Add(AttachmentAddArgs {
                owner_kind: "project".into(),
                owner_id: project.id.clone(),
                path: source.clone(),
                description: None,
            }),
        )
        .unwrap();
        let value = run_local(
            &db,
            AttachmentSubcommand::List(AttachmentListArgs {
                owner_kind: "project".into(),
                owner_id: project.id,
            }),
        )
        .unwrap();
        assert_eq!(value["accepted"].as_array().unwrap().len(), 1);
        assert_eq!(value["staged"].as_array().unwrap().len(), 0);
        assert_eq!(value["accepted"][0]["filename"], "list.txt");
    }

    #[test]
    fn local_flush_is_refused_with_connected_only_message() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("local.db");
        let error = run_local(&db, AttachmentSubcommand::Flush).unwrap_err();
        assert!(matches!(error, CliError::Usage(message) if message.contains("connected")));
    }

    #[test]
    fn pending_rows_are_labelled_pending_in_human_output() {
        let attachment = AttachmentDto {
            id: "attachment-1".into(),
            owner_kind: "task".into(),
            owner_id: "task-1".into(),
            filename: "draft.txt".into(),
            content_type: "text/plain".into(),
            size_bytes: 5,
            checksum: "a".repeat(64),
            created_at_ms: 1,
            description: None,
            is_image: false,
        };
        let value = json!({
            "accepted": [],
            "staged": [{
                "attachment": attachment_json(&attachment),
                "state": "pending",
                "last_error": "server unreachable",
            }],
        });
        let text = human_text(&value);
        assert!(text.contains("Pending uploads: 1"), "{text}");
        assert!(text.contains("[pending]"), "{text}");
        assert!(text.contains("server unreachable"), "{text}");
    }
}
