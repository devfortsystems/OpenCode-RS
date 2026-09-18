//! Test integracyjny: czy `opencode acp` (oryginalny opencode) odpowiada przez ACP.
//! Oryginalny opencode v1.18.21 ma 127 modeli w tym darmowe (ling, mimo, nemotron).
//! Nie wymaga wtyczki — to CLI.

use opencode_rs::providers::acp::AcpClientProvider;
use opencode_rs::providers::{ChatMessage, Provider};
use tokio::sync::mpsc;

async fn run_opencode_acp(model: Option<&str>, prompt: &str) -> String {
    if std::process::Command::new("opencode").arg("--version").output().is_err() {
        return "SKIP: opencode not available".to_string();
    }

    let provider = AcpClientProvider::opencode(model, std::env::current_dir().unwrap());
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: prompt.to_string(),
    }];

    let handle = tokio::spawn(async move {
        provider.stream_chat("opencode-acp", &messages, tx).await
    });

    let mut full_output = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(chunk)) => {
                eprint!("{}", chunk);
                full_output.push_str(&chunk);
                if full_output.contains("stop_reason") {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => {
                handle.abort();
                break;
            }
        }
    }

    let _ = handle.await;
    full_output
}

#[tokio::test]
#[ignore]
async fn test_opencode_acp_default_model() {
    eprintln!("🚀 Test: opencode acp (domyślny model)");

    let output = run_opencode_acp(
        None,
        "Jaka jest stolica Polski? Odpowiedz jednym słowem.",
    )
    .await;

    if output.starts_with("SKIP") {
        eprintln!("⏭️  {output}");
        return;
    }

    eprintln!("\n=== OUTPUT ===\n{output}\n=== KONIEC ===\n");

    assert!(!output.is_empty(), "opencode acp powinien coś wypisać");
    let lower = output.to_lowercase();
    assert!(
        lower.contains("warszawa") || lower.contains("warsaw"),
        "opencode acp powinien odpowiedzieć 'Warszawa'. Got:\n{output}"
    );

    eprintln!("✅ opencode acp (domyślny) poprawnie odpowiedział");
}

#[tokio::test]
async fn test_opencode_acp_free_model() {
    eprintln!("🚀 Test: opencode acp → darmowy Ling 3.0 Flash");

    let output = run_opencode_acp(
        Some("opencode/ling-3.0-flash-fin-free"),
        "Ile to 2+2? Odpowiedz tylko liczbą.",
    )
    .await;

    if output.starts_with("SKIP") {
        eprintln!("⏭️  {output}");
        return;
    }

    eprintln!("\n=== OUTPUT ===\n{output}\n=== KONIEC ===\n");

    assert!(!output.is_empty(), "Ling free powinien coś wypisać");
    let lower = output.to_lowercase();
    assert!(
        lower.contains('4') || lower.contains("cztery") || lower.contains("four"),
        "Ling free powinien odpowiedzieć '4'. Got:\n{output}"
    );

    eprintln!("✅ opencode acp → Ling 3.0 Flash (darmowy) poprawnie odpowiedział");
}
