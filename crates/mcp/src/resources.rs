//! Built-in reference documents served through MCP resources.

use crate::proto::Error;
use serde_json::{Value, json};

pub fn list(params: Option<&Value>) -> Result<Value, Error> {
    if params
        .and_then(|params| params.get("cursor"))
        .is_some_and(|cursor| !cursor.is_null())
    {
        return Err(Error::invalid_params(
            "unknown resource cursor; omit it to list all resources",
        ));
    }
    Ok(json!({
        "resources": prompts::resources::list().iter().map(|resource| json!({
            "uri": resource.uri(),
            "name": resource.name,
            "description": resource.description,
            "mimeType": "text/markdown",
            "size": resource.content.len(),
        })).collect::<Vec<_>>()
    }))
}

pub fn read(params: Option<&Value>) -> Result<Value, Error> {
    let uri = params
        .and_then(|params| params.get("uri"))
        .and_then(Value::as_str)
        .ok_or_else(|| Error::invalid_params("uri is required, as a string"))?;
    let resource = uri
        .strip_prefix("cydonia://resources/")
        .and_then(prompts::resources::read)
        .ok_or_else(|| {
            Error::new(
                crate::proto::RESOURCE_NOT_FOUND,
                format!("no resource {uri}"),
            )
        })?;
    Ok(json!({ "contents": [{
        "uri": resource.uri(),
        "mimeType": "text/markdown",
        "text": resource.content,
    }] }))
}
