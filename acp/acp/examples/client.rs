use cydonia_acp::{Client, Result, schema};
use std::sync::Arc;
use tokio::process::Command;

struct Ui;

impl Client for Ui {
    async fn session_update(&self, notification: schema::SessionNotification) {
        if let schema::SessionUpdate::AgentMessageChunk(chunk) = notification.update {
            print!("{:?}", chunk.content);
        }
    }

    async fn request_permission(
        &self,
        request: schema::RequestPermissionRequest,
    ) -> Result<schema::RequestPermissionResponse> {
        Ok(schema::RequestPermissionResponse::selected(
            request.options[0].option_id.clone(),
        ))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let (agent, _child) =
        cydonia_acp::spawn(Command::new("my-agent").arg("--acp"), Arc::new(Ui), None)?;

    agent
        .initialize(schema::InitializeRequest::new(Default::default()))
        .await?;
    let session = agent
        .new_session(schema::NewSessionRequest::new("/path/to/repo"))
        .await?;
    let done = agent
        .prompt(schema::PromptRequest::new(
            session.session_id,
            vec!["explain this repo".into()],
        ))
        .await?;

    println!("{:?}", done.stop_reason);
    Ok(())
}
