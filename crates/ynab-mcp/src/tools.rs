use ynab_core::{OutputEnvelope, Result as YnabResult, YnabError};

pub fn render_json(result: YnabResult<OutputEnvelope>) -> Result<String, String> {
    let envelope = result.map_err(to_tool_error)?;
    serde_json::to_string(&envelope.data).map_err(|error| error.to_string())
}

pub fn to_tool_error(error: YnabError) -> String {
    let envelope = error.to_cli_envelope();
    serde_json::to_string(&envelope).unwrap_or(envelope.error.message)
}
