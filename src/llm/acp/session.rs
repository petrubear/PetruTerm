//! The ACP protocol handler chain: wires agent notifications/requests
//! (session/update, permission, fs/*, terminal/*) to `AiEvent`s and
//! `AcpTerminalRequest`s, and drives the prompt request/response loop.

use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::schema::{
    ContentBlock, CreateTerminalRequest, InitializeRequest, KillTerminalRequest,
    KillTerminalResponse, NewSessionRequest, PromptRequest, ProtocolVersion, ReadTextFileRequest,
    ReadTextFileResponse, ReleaseTerminalRequest, ReleaseTerminalResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, TerminalExitStatus, TerminalId,
    TerminalOutputRequest, TerminalOutputResponse, WaitForTerminalExitRequest,
    WaitForTerminalExitResponse, WriteTextFileRequest, WriteTextFileResponse,
};
use agent_client_protocol::{Agent, ConnectionTo};
use agent_client_protocol_tokio::AcpAgent;
use anyhow::Result;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::llm::chat_panel::{AiEvent, ConfirmDisplay};

use super::fs::validate_path;
use super::terminal::AcpTerminalRequest;
use super::{PromptMsg, QueryCtx, TermCtx};

pub(super) async fn run_session(
    agent: AcpAgent,
    cwd: PathBuf,
    mut prompt_rx: mpsc::Receiver<PromptMsg>,
    ready_tx: oneshot::Sender<Result<()>>,
) {
    let query_ctx: QueryCtx = Arc::new(Mutex::new(None));
    let term_ctx: TermCtx = Arc::new(Mutex::new(None));

    // Clones for the notification/request handlers (all 'static + Send).
    let qc_notif = query_ctx.clone();
    let qc_perm = query_ctx.clone();
    let qc_write = query_ctx.clone();
    let tc_create = term_ctx.clone();
    let tc_output = term_ctx.clone();
    let tc_wait = term_ctx.clone();
    let tc_kill = term_ctx.clone();
    let tc_release = term_ctx.clone();

    let mut ready_tx = Some(ready_tx);

    let result = agent_client_protocol::Client
        .builder()
        // ── session/update notifications → AiEvent::Token / ToolStatus ──────
        .on_receive_notification(
            async move |notif: SessionNotification, _cx| {
                let tx = { qc_notif.lock().clone() };
                let Some(tx) = tx else { return Ok(()) };

                match notif.update {
                    SessionUpdate::AgentMessageChunk(chunk) => {
                        if let ContentBlock::Text(t) = chunk.content {
                            let _ = tx.send(AiEvent::Token(t.text)).await;
                        }
                    }
                    SessionUpdate::ToolCall(tc) => {
                        let _ = tx
                            .send(AiEvent::ToolStatus {
                                tool: tc.title,
                                path: String::new(),
                                done: false,
                            })
                            .await;
                    }
                    SessionUpdate::ToolCallUpdate(tcu) => {
                        if let Some(title) = tcu.fields.title {
                            let _ = tx
                                .send(AiEvent::ToolStatus {
                                    tool: title,
                                    path: String::new(),
                                    done: true,
                                })
                                .await;
                        }
                    }
                    _ => {}
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        // ── session/requestPermission → AiEvent::ConfirmRun ──────────────────
        .on_receive_request(
            async move |req: RequestPermissionRequest, responder, _cx| {
                let tx = { qc_perm.lock().clone() };
                let outcome = if let Some(tx) = tx {
                    let label = req
                        .options
                        .iter()
                        .map(|o| o.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" / ");
                    let (result_tx, result_rx) = oneshot::channel::<bool>();
                    let _ = tx
                        .send(AiEvent::ConfirmRun {
                            cmd: label,
                            result_tx,
                        })
                        .await;
                    let approved = result_rx.await.unwrap_or(false);
                    if approved {
                        req.options
                            .first()
                            .map(|o| {
                                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                    o.option_id.clone(),
                                ))
                            })
                            .unwrap_or(RequestPermissionOutcome::Cancelled)
                    } else {
                        RequestPermissionOutcome::Cancelled
                    }
                } else {
                    RequestPermissionOutcome::Cancelled
                };
                let _ = responder.respond(RequestPermissionResponse::new(outcome));
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── fs/write_text_file → AiEvent::ConfirmWrite ───────────────────────
        .on_receive_request(
            async move |req: WriteTextFileRequest, responder, _cx| {
                let tx = { qc_write.lock().clone() };
                if let Some(tx) = tx {
                    match validate_path(&req.path) {
                        Ok(safe_path) => {
                            let display = ConfirmDisplay::for_write(&safe_path, &req.content);
                            let (result_tx, result_rx) = oneshot::channel::<bool>();
                            let _ = tx.send(AiEvent::ConfirmWrite { display, result_tx }).await;
                            if result_rx.await.unwrap_or(false) {
                                let orig = tokio::fs::read_to_string(&safe_path)
                                    .await
                                    .unwrap_or_default();
                                let content = req.content.clone();
                                if let Err(e) = tokio::fs::write(&safe_path, content).await {
                                    log::error!(
                                        "ACP fs/write_text_file {}: {e}",
                                        safe_path.display()
                                    );
                                } else {
                                    let _ = tx
                                        .send(AiEvent::UndoState {
                                            path: safe_path,
                                            content: orig,
                                        })
                                        .await;
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!("ACP fs/write_text_file path rejected: {e}");
                        }
                    }
                }
                let _ = responder.respond(WriteTextFileResponse::new());
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── fs/read_text_file — read-only, no confirmation needed ────────────
        .on_receive_request(
            async move |req: ReadTextFileRequest, responder, _cx| {
                let content = match validate_path(&req.path) {
                    Ok(safe_path) => tokio::fs::read_to_string(&safe_path)
                        .await
                        .unwrap_or_default(),
                    Err(e) => {
                        log::warn!("ACP fs/read_text_file path rejected: {e}");
                        String::new()
                    }
                };
                let _ = responder.respond(ReadTextFileResponse::new(content));
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── terminal/create ───────────────────────────────────────────────────
        .on_receive_request(
            async move |req: CreateTerminalRequest, responder, _cx| {
                let tx = { tc_create.lock().clone() };
                if let Some(tx) = tx {
                    let (pane_tx, pane_rx) = oneshot::channel::<usize>();
                    let _ = tx
                        .send(AcpTerminalRequest::Create {
                            command: req.command,
                            args: req.args,
                            cwd: req.cwd,
                            tx: pane_tx,
                        })
                        .await;
                    if let Ok(pane_id) = pane_rx.await {
                        let _ = responder.respond(
                            agent_client_protocol::schema::CreateTerminalResponse::new(
                                TerminalId::new(pane_id.to_string()),
                            ),
                        );
                        return Ok(());
                    }
                }
                // Fallback: return a dummy ID so the agent doesn't hang.
                let _ =
                    responder.respond(agent_client_protocol::schema::CreateTerminalResponse::new(
                        TerminalId::new("0"),
                    ));
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── terminal/output ───────────────────────────────────────────────────
        .on_receive_request(
            async move |req: TerminalOutputRequest, responder, _cx| {
                let pane_id = req.terminal_id.0.parse::<usize>().unwrap_or(0);
                let tx = { tc_output.lock().clone() };
                if let Some(tx) = tx {
                    let (out_tx, out_rx) = oneshot::channel::<(String, Option<i32>)>();
                    let _ = tx
                        .send(AcpTerminalRequest::GetOutput {
                            pane_id,
                            tx: out_tx,
                        })
                        .await;
                    if let Ok((output, exit_code)) = out_rx.await {
                        let mut resp = TerminalOutputResponse::new(output, false);
                        if let Some(code) = exit_code {
                            resp = resp.exit_status(
                                TerminalExitStatus::new().exit_code(u32::try_from(code).ok()),
                            );
                        }
                        let _ = responder.respond(resp);
                        return Ok(());
                    }
                }
                let _ = responder.respond(TerminalOutputResponse::new(String::new(), false));
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── terminal/wait_for_exit ────────────────────────────────────────────
        .on_receive_request(
            async move |req: WaitForTerminalExitRequest, responder, _cx| {
                let pane_id = req.terminal_id.0.parse::<usize>().unwrap_or(0);
                let tx = { tc_wait.lock().clone() };
                let exit_code: i32 = if let Some(tx) = tx {
                    let (ex_tx, ex_rx) = oneshot::channel::<i32>();
                    let _ = tx
                        .send(AcpTerminalRequest::WaitForExit { pane_id, tx: ex_tx })
                        .await;
                    ex_rx.await.unwrap_or(1)
                } else {
                    1
                };
                let _ = responder.respond(WaitForTerminalExitResponse::new(
                    TerminalExitStatus::new().exit_code(u32::try_from(exit_code).ok()),
                ));
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── terminal/kill ─────────────────────────────────────────────────────
        .on_receive_request(
            async move |req: KillTerminalRequest, responder, _cx| {
                let pane_id = req.terminal_id.0.parse::<usize>().unwrap_or(0);
                let tx = { tc_kill.lock().clone() };
                if let Some(tx) = tx {
                    let _ = tx.send(AcpTerminalRequest::Kill { pane_id }).await;
                }
                let _ = responder.respond(KillTerminalResponse::new());
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── terminal/release — pane stays visible, user closes manually ───────
        .on_receive_request(
            async move |_req: ReleaseTerminalRequest, responder, _cx| {
                let _ = tc_release; // suppress unused warning — release is a no-op
                let _ = responder.respond(ReleaseTerminalResponse::new());
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async |cx: ConnectionTo<Agent>| {
            cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;

            let sess = cx
                .send_request(NewSessionRequest::new(&cwd))
                .block_task()
                .await?;
            let session_id = sess.session_id;

            if let Some(tx) = ready_tx.take() {
                let _ = tx.send(Ok(()));
            }

            while let Some(msg) = prompt_rx.recv().await {
                // Wire the current query's channels into the shared contexts.
                *query_ctx.lock() = Some(msg.ai_tx.clone());
                *term_ctx.lock() = Some(msg.terminal_tx.clone());

                let res = cx
                    .send_request(PromptRequest::new(
                        session_id.clone(),
                        vec![ContentBlock::Text(
                            agent_client_protocol::schema::TextContent::new(msg.content),
                        )],
                    ))
                    .block_task()
                    .await;

                *query_ctx.lock() = None;
                *term_ctx.lock() = None;

                match res {
                    Ok(_) => {
                        let _ = msg.ai_tx.send(AiEvent::Done).await;
                    }
                    Err(e) => {
                        let _ = msg.ai_tx.send(AiEvent::Error(e.to_string())).await;
                    }
                }
            }

            Ok(())
        })
        .await;

    if let Err(e) = result {
        log::error!("ACP session ended with error: {e}");
    }
}
