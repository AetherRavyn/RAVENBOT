//! Shared approve/deny/answer handling for the native chat adapters.
//!
//! A chat cannot render the desktop approval card, so the notice includes the
//! request id and accepts explicit commands. Every request is checked against
//! the paired thread before it can be decided.

use ravenbot_db::queries::{ApprovalQueries, QuestionQueries};
use ravenbot_db::Database;
use ravenbot_runtime::Runtime;
use std::sync::Arc;
use uuid::Uuid;

/// Attribute a run started by an external chat user. The workspace comes from
/// the account process environment; the actor is the platform's user identity.
pub async fn attribute_run(
    db: &Database,
    run_id: Uuid,
    actor_id: &str,
    actor_name: &str,
) -> Result<(), String> {
    let workspace =
        std::env::var("RAVENBOT_WORKSPACE_ID").unwrap_or_else(|_| "workspace".to_string());
    ravenbot_db::queries::RunQueries::set_actor(
        db.pool(),
        run_id,
        actor_id,
        actor_name,
        "chat",
        &workspace,
    )
    .await
    .map_err(|error| error.to_string())
}

pub async fn decide_approval(
    db: &Database,
    runtime: &Arc<Runtime>,
    thread_id: Uuid,
    approval_id: Uuid,
    allowed: bool,
) -> Result<String, String> {
    let request = ApprovalQueries::get(db.pool(), approval_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "That approval no longer exists.".to_string())?;
    if request.thread_id != thread_id {
        return Err("That approval belongs to another conversation.".to_string());
    }
    let decided = ApprovalQueries::decide(db.pool(), approval_id, allowed, None)
        .await
        .map_err(|error| error.to_string())?;
    if !decided {
        return Err("That approval was already decided.".to_string());
    }
    let permission = format!("tool:{}", request.tool_name);
    let event = if allowed {
        ravenbot_core::AuditEventType::PermissionGranted { permission }
    } else {
        ravenbot_core::AuditEventType::PermissionDenied {
            permission,
            reason: "Denied from the paired chat".to_string(),
        }
    };
    let entry = ravenbot_core::AuditEntry::new(request.bot_id, event)
        .with_run_id(request.run_id)
        .with_thread_id(request.thread_id);
    let _ = runtime.audit_logger().log(&entry).await;
    Ok(if allowed {
        "Approved.".to_string()
    } else {
        "Denied.".to_string()
    })
}

pub async fn answer_question(
    db: &Database,
    thread_id: Uuid,
    question_id: Uuid,
    answer: &str,
) -> Result<String, String> {
    let question = QuestionQueries::get(db.pool(), question_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "That question no longer exists.".to_string())?;
    if question.thread_id != thread_id {
        return Err("That question belongs to another conversation.".to_string());
    }
    let answered = QuestionQueries::answer(db.pool(), question_id, answer)
        .await
        .map_err(|error| error.to_string())?;
    if !answered {
        return Err("That question was already answered.".to_string());
    }
    Ok("Answer recorded.".to_string())
}

#[derive(Debug, Clone)]
pub enum PendingCard {
    Approval {
        id: Uuid,
        title: String,
        detail: String,
        risk: String,
    },
    Question {
        id: Uuid,
        header: String,
        question: String,
        options: Vec<String>,
    },
}

pub async fn pending_card(db: &Database, thread_id: Uuid) -> Option<PendingCard> {
    let approvals = ApprovalQueries::list_pending_for_thread(db.pool(), thread_id)
        .await
        .ok()?;
    if let Some(request) = approvals.first() {
        return Some(PendingCard::Approval {
            id: request.id,
            title: request.tool_label.clone(),
            detail: format!("Tool: {}", request.tool_name),
            risk: request.risk.clone(),
        });
    }
    let questions = QuestionQueries::list_pending_for_thread(db.pool(), thread_id)
        .await
        .ok()?;
    let question = questions.first()?;
    Some(PendingCard::Question {
        id: question.id,
        header: question.header.clone(),
        question: question.question.clone(),
        options: question.options.clone(),
    })
}

pub async fn answer_option(
    db: &Database,
    thread_id: Uuid,
    question_id: Uuid,
    index: usize,
) -> Result<String, String> {
    let question = QuestionQueries::get(db.pool(), question_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "That question no longer exists.".to_string())?;
    if question.thread_id != thread_id {
        return Err("That question belongs to another conversation.".to_string());
    }
    let option = question
        .options
        .get(index)
        .ok_or_else(|| "That option no longer exists.".to_string())?;
    let answered = QuestionQueries::answer(db.pool(), question_id, option)
        .await
        .map_err(|error| error.to_string())?;
    if !answered {
        return Err("That question was already answered.".to_string());
    }
    Ok(format!("Answer recorded: {option}"))
}

pub fn parse_card_action(data: &str) -> Option<(String, Uuid, Option<usize>)> {
    let mut parts = data.split(':');
    let action = parts.next()?.to_string();
    let id = Uuid::parse_str(parts.next()?).ok()?;
    let index = parts.next().and_then(|value| value.parse::<usize>().ok());
    Some((action, id, index))
}

pub async fn pending_notice(db: &Database, thread_id: Uuid) -> Option<String> {
    let approvals = ApprovalQueries::list_pending_for_thread(db.pool(), thread_id)
        .await
        .ok()?;
    if let Some(request) = approvals.first() {
        let more = approvals.len().saturating_sub(1);
        return Some(format!(
            "Approval required: {} ({}){}. Reply `approve {}` or `deny {}`.",
            request.tool_label,
            request.risk,
            if more > 0 {
                format!(", and {more} more pending")
            } else {
                String::new()
            },
            request.id,
            request.id
        ));
    }
    let questions = QuestionQueries::list_pending_for_thread(db.pool(), thread_id)
        .await
        .ok()?;
    let question = questions.first()?;
    Some(format!(
        "Question: {}{}. Reply `answer {} <text>`.",
        question.header,
        if question.question.trim().is_empty() {
            String::new()
        } else {
            format!(" — {}", question.question)
        },
        question.id
    ))
}

pub async fn first_pending_approval(
    db: &Database,
    thread_id: Uuid,
) -> Result<Option<Uuid>, String> {
    Ok(
        ApprovalQueries::list_pending_for_thread(db.pool(), thread_id)
            .await
            .map_err(|error| error.to_string())?
            .first()
            .map(|request| request.id),
    )
}

pub async fn first_pending_question(
    db: &Database,
    thread_id: Uuid,
) -> Result<Option<Uuid>, String> {
    Ok(
        QuestionQueries::list_pending_for_thread(db.pool(), thread_id)
            .await
            .map_err(|error| error.to_string())?
            .first()
            .map(|question| question.id),
    )
}

/// Handle an approval or question command for a paired thread. Returns `None`
/// when the text is not one of those commands.
pub async fn handle(
    db: &Database,
    runtime: &Arc<Runtime>,
    thread_id: Uuid,
    text: &str,
) -> Option<Result<String, String>> {
    let command = text.split_whitespace().next().unwrap_or("");
    match command {
        "approve" | "deny" => Some(
            async {
                let id = match parse_id(text, 1) {
                    Some(id) => id,
                    None => first_pending_approval(db, thread_id)
                        .await?
                        .ok_or_else(|| "There is no pending approval.".to_string())?,
                };
                decide_approval(db, runtime, thread_id, id, command == "approve").await
            }
            .await,
        ),
        "answer" => Some(
            async {
                let explicit = parse_id(text, 1);
                let answer = if explicit.is_some() {
                    text.split_whitespace()
                        .skip(2)
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    text.split_whitespace()
                        .skip(1)
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                if answer.trim().is_empty() {
                    return Err("Send the answer after `answer`.".to_string());
                }
                let id = match explicit {
                    Some(id) => id,
                    None => first_pending_question(db, thread_id)
                        .await?
                        .ok_or_else(|| "There is no pending question.".to_string())?,
                };
                answer_question(db, thread_id, id, answer.trim()).await
            }
            .await,
        ),
        _ => None,
    }
}

pub fn parse_id(text: &str, index: usize) -> Option<Uuid> {
    Uuid::parse_str(text.split_whitespace().nth(index)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_parsed_from_the_expected_position() {
        let id = Uuid::new_v4();
        assert_eq!(parse_id(&format!("approve {id}"), 1), Some(id));
        assert_eq!(parse_id("approve nope", 1), None);
    }
}
