pub mod auth;
pub mod client;
pub mod framing;
pub mod mime;
pub mod parsing;

/// Escargot accounts are `username@escargot.chat`, but the website accepts the
/// bare username. The MSNP layer needs the full passport address.
pub fn normalize_account(input: &str) -> String {
    let input = input.trim();
    if input.is_empty() {
        return String::new();
    }

    let account = match input.rsplit_once('@') {
        Some((local, domain)) if domain.is_empty() => format!("{local}@escargot.chat"),
        Some(_) => input.to_string(),
        None => format!("{input}@escargot.chat"),
    };

    account.to_ascii_lowercase()
}

/// Normalize a contact address and reject values that cannot be one wire argument.
pub fn normalize_contact_account(input: &str) -> Result<String, String> {
    let account = normalize_account(input);
    let valid = account.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && !domain.is_empty() && !domain.contains('@')
    });
    if !valid || account.len() > 320 || account.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err("Enter a valid Escargot username or email.".into());
    }
    Ok(account)
}
