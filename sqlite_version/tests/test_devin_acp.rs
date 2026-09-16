//! Test integracyjny ACP — uruchamia `devin acp` przez AcpClientProvider z prostym promptem.
//! Wymaga: devin CLI zainstalowane + zalogowane (devin auth status = Logged in).
//! Pomijany automatycznie jeśli devin nie jest dostępny.
//!
//! Uruchom: cargo test --test test_devin_acp -- --nocapture --ignored

use opencode_rs::providers::acp::AcpClientProvider;
use opencode_rs::providers::{ChatMessage, Provider};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_devin_acp_simple_prompt() {
    // Sprawdź czy devin jest dostępny
    if std::process::Command::new("devin").arg("--version").output().is_err() {
        eprintln!("⏭️  Pominięto: devin CLI nie jest dostępny w PATH");
        return;
    }

    eprintln!("🚀 Test ACP: uruchamianie devin acp z prostym promptem...");

    let provider = AcpClientProvider::devin(None, std::env::current_dir().unwrap());
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: "Say exactly: ACP_TEST_OK".to_string(),
    }];

    // Uruchom provider w tasku — streamuje do tx
    let handle = tokio::spawn(async move {
        provider.stream_chat("devin-acp", &messages, tx).await
    });

    // Zbierz output z timeoutem 60s (ACP handshake + prompt może chwilę potrwać)
    let mut full_output = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(chunk)) => {
                eprint!("{}", chunk);
                full_output.push_str(&chunk);
                // Jeśli widzimy zakończenie turn, przerwij
                if full_output.contains("stop_reason") {
                    break;
                }
            }
            Ok(None) => break, // channel zamknięty = provider skończył
            Err(_) => {
                eprintln!("\n⏰ Timeout 90s — przerywam");
                break;
            }
        }
    }

    // Poczekaj na zakończenie tasku
    let result = handle.await;
    eprintln!("\n📋 Provider result: {:?}", result);

    // Asercje: powinien być jakiś output (handshake + odpowiedź)
    assert!(!full_output.is_empty(), "ACP provider powinien coś wypisać");
    assert!(
        full_output.contains("ACP") || full_output.contains("połączono"),
        "Powinien być nagłówek ACP, got: {full_output}"
    );
}

#[tokio::test]
async fn test_devin_acp_with_gemini_model() {
    if std::process::Command::new("devin").arg("--version").output().is_err() {
        eprintln!("⏭️  Pominięto: devin CLI nie jest dostępny");
        return;
    }

    eprintln!("🚀 Test ACP z modelem gemini...");

    let provider = AcpClientProvider::devin(Some("gemini"), std::env::current_dir().unwrap());
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: "What is 2+2? Reply with just the number.".to_string(),
    }];

    let handle = tokio::spawn(async move {
        provider.stream_chat("devin-acp", &messages, tx).await
    });

    let mut full_output = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(90);
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
                eprintln!("\n⏰ Timeout");
                break;
            }
        }
    }

    let _ = handle.await;
    assert!(!full_output.is_empty(), "ACP powinien coś wypisać");
    eprintln!("\n📋 Output końcowy ({} znaków)", full_output.len());
}
