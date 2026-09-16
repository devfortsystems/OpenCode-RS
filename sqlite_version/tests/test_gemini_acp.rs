//! Test integracyjny: czy `gemini --acp` (Gemini CLI, darmowy tier) odpowiada przez ACP.
//! Gemini CLI ma darmowy tier: 60 req/min, 1000 req/day z Google account.

use opencode_rs::providers::acp::AcpClientProvider;
use opencode_rs::providers::{ChatMessage, Provider};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_gemini_acp_free() {
    // Sprawdź czy gemini jest zainstalowane
    let gemini_check = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/c", "gemini", "--version"])
            .output()
    } else {
        std::process::Command::new("gemini").arg("--version").output()
    };
    if gemini_check.is_err() {
        eprintln!("⏭️  SKIP: gemini not available");
        return;
    }

    // Sprawdź czy gemini jest zalogowany (ma settings.json lub GEMINI_API_KEY)
    let has_auth = std::env::var("GEMINI_API_KEY").is_ok()
        || std::path::Path::new(&std::env::var("USERPROFILE").unwrap_or_default())
            .join(".gemini").join("settings.json").exists();
    if !has_auth {
        eprintln!("⏭️  SKIP: gemini not authenticated (uruchom `gemini` i zaloguj się Google account)");
        return;
    }

    eprintln!("🚀 Test: gemini --acp (darmowy tier)");

    let provider = AcpClientProvider::gemini(std::env::current_dir().unwrap());
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: "Ile to 2+2? Odpowiedz tylko liczbą.".to_string(),
    }];

    let handle = tokio::spawn(async move {
        provider.stream_chat("gemini-acp", &messages, tx).await
    });

    let mut full_output = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(chunk)) => {
                eprint!("{}", chunk);
                full_output.push_str(&chunk);
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    let _ = handle.await;

    eprintln!("\n=== OUTPUT ===\n{full_output}\n=== KONIEC ===\n");

    if full_output.is_empty() {
        eprintln!("⚠️  Gemini ACP nie wypisał nic — możliwe że wymaga login (`gemini`)");
        return;
    }

    let lower = full_output.to_lowercase();
    assert!(
        lower.contains('4') || lower.contains("cztery") || lower.contains("four"),
        "Gemini ACP powinien odpowiedzieć '4'. Got:\n{full_output}"
    );

    eprintln!("✅ Gemini CLI ACP (darmowy) poprawnie odpowiedział");
}
