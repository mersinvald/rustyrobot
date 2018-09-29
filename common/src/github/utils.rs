use gh4;
use gh3;

use json::Value;

pub fn is_rate_limit_error_v4(status: gh4::StatusCode, body: &Value) -> bool {
    match status {
        gh4::StatusCode::Forbidden => (),
        _ => return false,
    }

    is_body_rate_limit_error(body)
}

pub fn is_rate_limit_error_v3(status: gh3::StatusCode, body: &Value) -> bool {
    match status {
        gh3::StatusCode::Forbidden => (),
        _ => return false,
    }

    is_body_rate_limit_error(body)
}

pub fn get_error_message(body: &Value) -> Option<&str> {
    body.get("message").and_then(|v| v.as_str())
}

fn is_body_rate_limit_error(body: &Value) -> bool {
    let message = get_error_message(body);
    message.map(|s| s.starts_with("API rate limit exceeded"))
        .unwrap_or(false)
}

use std::env;
use failure::Error;
use dotenv;

pub fn load_token() -> Result<String, Error> {
    // First search .env
    let token = dotenv::var("GITHUB_TOKEN")
        // Then environment variables
        .or_else(|_| {
            env::var("GITHUB_TOKEN")
        })?;

    Ok(token)
}
