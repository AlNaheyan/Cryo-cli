use anyhow::Result;
use ffsend_api::action::exists::Exists;
use ffsend_api::client::Client;
use ffsend_api::file::remote_file::RemoteFile;

/// Resolve the password to use for a read.
/// If `provided` is set, use it. Otherwise check whether the file is
/// password-protected and, only if so, prompt securely (no echo).
pub fn resolve(
    file: &RemoteFile,
    client: &Client,
    provided: Option<String>,
) -> Result<Option<String>> {
    if provided.is_some() {
        return Ok(provided);
    }
    let res = Exists::new(file).invoke(client)?;
    if res.requires_password() {
        let pw = rpassword::prompt_password("File password: ")?;
        Ok(Some(pw))
    } else {
        Ok(None)
    }
}
