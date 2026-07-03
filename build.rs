use std::path::Path;

fn main() {
    let secrets_path = Path::new("google_secrets.json");
    if secrets_path.exists()
        && let Ok(content) = std::fs::read_to_string(secrets_path)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
    {
        if let Some(client_id) = json.get("client_id").and_then(|v| v.as_str()) {
            println!("cargo:rustc-env=GOOGLE_CLIENT_ID={client_id}");
        }
        if let Some(client_secret) = json.get("client_secret").and_then(|v| v.as_str()) {
            println!("cargo:rustc-env=GOOGLE_CLIENT_SECRET={client_secret}");
        }
    } else if !secrets_path.exists() {
        println!("cargo:warning=google_secrets.json not found — meeting sync will be unavailable");
    }
    println!("cargo:rerun-if-changed=google_secrets.json");
}
