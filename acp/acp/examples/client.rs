use cydonia_acp::{Client, Result, proto};
use std::sync::Arc;
use tokio::process::Command;

struct Ui;

impl Client for Ui {
    async fn session_update(&self, notification: proto::SessionNotification) {
        if let proto::SessionUpdate::AgentMessageChunk(chunk) = notification.update {
            print!("{:?}", chunk.content);
        }
    }

    async fn request_permission(
        &self,
        request: proto::RequestPermissionRequest,
    ) -> Result<proto::RequestPermissionResponse> {
        Ok(proto::RequestPermissionResponse::selected(
            request.options[0].option_id.clone(),
        ))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let (agent, _child) =
        cydonia_acp::spawn(Command::new("my-agent").arg("--acp"), Arc::new(Ui), None)?;

    agent
        .initialize(proto::InitializeRequest::new(Default::default()))
        .await?;
    let session = agent
        .new_session(proto::NewSessionRequest::new("/path/to/repo"))
        .await?;
    let done = agent
        .prompt(proto::PromptRequest::new(
            session.session_id,
            vec!["explain this repo".into()],
        ))
        .await?;

    println!("{:?}", done.stop_reason);
    Ok(())
}
