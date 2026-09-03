//! Test integracyjny: czy `kilo run -m <model>` (Kilo Code, fork opencode) odpowiada.
//! Kilo ma 302 modele, 17 darmowych (nvidia nemotron, minimax, ling, poolside, etc.).

use opencode_rs::providers::cli_subprocess::{CliSpec, CliSubprocessProvider};
use opencode_rs::providers::{ChatMessage, Provider};
use tokio::sync::mpsc;

async fn run_kilo(model: Option<&str>, prompt: &str) -> String {
    // Na Windows, `.cmd` shims (Volta/npm) wymagają `cmd /c` do uruchomienia.
    let kilo_check = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/c", "kilo", "--version"])
            .output()
    } else {
        std::process::Command::new("kilo").arg("--version").output()
    };
    if kilo_check.is_err() {
        return "SKIP: kilo not available".to_string();
    }

    let provider = CliSubprocessProvider::new(CliSpec::kilo_run());
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: prompt.to_string(),
    }];

    let model_arg = model.unwrap_or("kilo-run").to_string();
    let handle = tokio::spawn(async move {
        provider.stream_chat(&model_arg, &messages, tx).await
    });

    let mut full_output = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
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
    full_output
}

#[tokio::test]
async fn test_kilo_run_free_nemotron() {
    eprintln!("🚀 Test: kilo run → darmowy Nemotron 3.5 Lightning");

    let output = run_kilo(
        Some("kilo/nvidia/nemotron-3.5-lightning:free"),
        "Ile to 2+2? Odpowiedz tylko liczbą.",
    )
    .await;

    if output.starts_with("SKIP") {
        eprintln!("⏭️  {output}");
        return;
    }

    eprintln!("\n=== OUTPUT ===\n{output}\n=== KONIEC ===\n");

    assert!(!output.is_empty(), "Kilo powinien coś wypisać");
    let lower = output.to_lowercase();
    assert!(
        lower.contains('4') || lower.contains("cztery") || lower.contains("four"),
        "Kilo free powinien odpowiedzieć '4'. Got:\n{output}"
    );

    eprintln!("✅ Kilo Code → Nemotron 3.5 Lightning (darmowy) poprawnie odpowiedział");
}
