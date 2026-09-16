//! Test: darmowe modele Devina (SWE-1.7, GLM-5.2) przez ACP.
//! SWE-1.7 = model od Cognition, zoptymalizowany pod coding, darmowy.
//! GLM-5.2 = model od Zhipu AI, darmowy.

use opencode_rs::providers::acp::AcpClientProvider;
use opencode_rs::providers::{ChatMessage, Provider};
use tokio::sync::mpsc;

async fn run_acp_test(model: &str, prompt: &str, _expect_substring: &str) -> String {
    if std::process::Command::new("devin").arg("--version").output().is_err() {
        return "SKIP: devin not available".to_string();
    }

    let provider = AcpClientProvider::devin(Some(model), std::env::current_dir().unwrap());
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: prompt.to_string(),
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
            Err(_) => break,
        }
    }

    let _ = handle.await;
    full_output
}

#[tokio::test]
async fn test_devin_acp_swe_free_model() {
    eprintln!("🚀 Test: darmowy model SWE-1.7 (swe-1-7)");

    let output = run_acp_test(
        "swe-1-7",
        "Jaka jest stolica Polski? Odpowiedz jednym słowem.",
        "warszawa",
    )
    .await;

    if output.starts_with("SKIP") {
        eprintln!("⏭️  {output}");
        return;
    }

    eprintln!("\n=== OUTPUT ===\n{output}\n=== KONIEC ===\n");

    assert!(!output.is_empty(), "SWE-1.7 powinien coś wypisać");
    let lower = output.to_lowercase();
    assert!(
        lower.contains("warszawa") || lower.contains("warsaw"),
        "SWE-1.7 powinien odpowiedzieć 'Warszawa'. Got:\n{output}"
    );

    eprintln!("✅ SWE-1.7 (darmowy) poprawnie odpowiedział");
}

#[tokio::test]
async fn test_devin_acp_swe_code_task() {
    eprintln!("🚀 Test: SWE-1.7 — zadanie kodowe (reverse string w Rust)");

    let output = run_acp_test(
        "swe-1-7",
        "Napisz funkcję reverse_string(s: &str) -> String w Rust. Tylko kod.",
        "fn reverse",
    )
    .await;

    if output.starts_with("SKIP") {
        eprintln!("⏭️  {output}");
        return;
    }

    eprintln!("\n=== OUTPUT ===\n{output}\n=== KONIEC ===\n");

    assert!(!output.is_empty(), "SWE-1.7 powinien coś wypisać");
    let lower = output.to_lowercase();
    assert!(
        lower.contains("fn reverse") || lower.contains("reverse_string"),
        "SWE-1.7 powinien napisać funkcję reverse_string. Got:\n{output}"
    );

    eprintln!("✅ SWE-1.7 wygenerował kod Rust");
}

#[tokio::test]
async fn test_devin_acp_glm_free_model() {
    eprintln!("🚀 Test: darmowy model GLM-5.2 (glm-5-2)");

    let output = run_acp_test(
        "glm-5-2",
        "Ile to 2+2? Odpowiedz tylko liczbą.",
        "4",
    )
    .await;

    if output.starts_with("SKIP") {
        eprintln!("⏭️  {output}");
        return;
    }

    eprintln!("\n=== OUTPUT ===\n{output}\n=== KONIEC ===\n");

    assert!(!output.is_empty(), "GLM-5.2 powinien coś wypisać");
    // GLM może odpowiedzieć "4" lub "Cztery" — sprawdzamy oba
    let lower = output.to_lowercase();
    assert!(
        lower.contains('4') || lower.contains("cztery") || lower.contains("four"),
        "GLM-5.2 powinien odpowiedzieć '4'. Got:\n{output}"
    );

    eprintln!("✅ GLM-5.2 (darmowy) poprawnie odpowiedział");
}

#[tokio::test]
async fn test_devin_acp_swe_medium_free() {
    eprintln!("🚀 Test: darmowy model SWE-1.7 Medium (swe-1-7-medium)");

    let output = run_acp_test(
        "swe-1-7-medium",
        "Co to jest stół? Odpowiedz krótko po polsku.",
        "",
    )
    .await;

    if output.starts_with("SKIP") {
        eprintln!("⏭️  {output}");
        return;
    }

    eprintln!("\n=== OUTPUT ===\n{output}\n=== KONIEC ===\n");

    assert!(!output.is_empty(), "SWE-1.7 Medium powinien coś wypisać");
    // Sprawdzamy że jest jakaś sensowna odpowiedź (nie pusta, nie błąd)
    assert!(
        !output.contains("error") || output.contains("stop_reason"),
        "SWE-1.7 Medium powinien odpowiedzieć bez błędu. Got:\n{output}"
    );

    eprintln!("✅ SWE-1.7 Medium (darmowy) poprawnie odpowiedział");
}
