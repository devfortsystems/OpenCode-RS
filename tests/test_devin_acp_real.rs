//! Test: czy Devin ACP faktycznie odpowiada na pytanie (nie tylko echo).
//! Pytamy o stolicę Polski — oczekujemy "Warszawa" w odpowiedzi.

use opencode_rs::providers::acp::AcpClientProvider;
use opencode_rs::providers::{ChatMessage, Provider};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_devin_acp_answers_real_question() {
    if std::process::Command::new("devin").arg("--version").output().is_err() {
        eprintln!("⏭️  Pominięto: devin CLI nie jest dostępny");
        return;
    }

    eprintln!("🚀 Test: pytanie do Devina ACP — 'Jaka jest stolica Polski?'");

    let provider = AcpClientProvider::devin(None, std::env::current_dir().unwrap());
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: "Jaka jest stolica Polski? Odpowiedz jednym słowem.".to_string(),
    }];

    let handle = tokio::spawn(async move {
        provider.stream_chat("devin-acp", &messages, tx).await
    });

    let mut full_output = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
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
                eprintln!("\n⏰ Timeout 120s");
                break;
            }
        }
    }

    let _ = handle.await;

    eprintln!("\n\n=== PEŁNY OUTPUT ===");
    eprintln!("{}", full_output);
    eprintln!("=== KONIEC ===\n");

    // Asercje
    assert!(!full_output.is_empty(), "ACP powinien coś wypisać");
    assert!(full_output.contains("ACP") || full_output.contains("połączono"), "Powinien być nagłówek");

    // Kluczowa asercja: czy odpowiedź zawiera "Warszawa" (lub "Warsaw")?
    // Devin może odpowiedzieć po polsku lub angielsku.
    let lower = full_output.to_lowercase();
    let has_answer = lower.contains("warszawa") || lower.contains("warsaw");
    assert!(
        has_answer,
        "Devin powinien odpowiedzieć 'Warszawa' lub 'Warsaw'. Got:\n{full_output}"
    );

    eprintln!("✅ Test przeszedł — Devin poprawnie odpowiedział na pytanie");
}

#[tokio::test]
async fn test_devin_acp_code_question() {
    if std::process::Command::new("devin").arg("--version").output().is_err() {
        eprintln!("⏭️  Pominięto: devin CLI nie jest dostępny");
        return;
    }

    eprintln!("🚀 Test: pytanie kodowe — 'napisz funkcję add w Rust'");

    let provider = AcpClientProvider::devin(None, std::env::current_dir().unwrap());
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: "Napisz prostą funkcję add(a, b) w Rust. Tylko kod, bez wyjaśnień.".to_string(),
    }];

    let handle = tokio::spawn(async move {
        provider.stream_chat("devin-acp", &messages, tx).await
    });

    let mut full_output = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
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

    eprintln!("\n\n=== PEŁNY OUTPUT ===");
    eprintln!("{}", full_output);
    eprintln!("=== KONIEC ===\n");

    assert!(!full_output.is_empty(), "ACP powinien coś wypisać");

    // Czy odpowiedź zawiera kod Rust? Szukamy "fn add" lub "fn add("
    let lower = full_output.to_lowercase();
    let has_code = lower.contains("fn add") || lower.contains("fn add(") || lower.contains("i32") || lower.contains("->");
    assert!(
        has_code,
        "Devin powinien napisać funkcję add w Rust. Got:\n{full_output}"
    );

    eprintln!("✅ Test przeszedł — Devin wygenerował kod Rust");
}
