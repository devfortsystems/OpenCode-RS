use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::Read;
use std::sync::{Arc, Mutex};

pub struct EmbeddedPty {
    pub buffer: Arc<Mutex<String>>,
}

impl EmbeddedPty {
    pub fn new() -> Self { Self { buffer: Arc::new(Mutex::new(String::new())) } }

    /// Uruchamia shell w PTY w tle — pełna implementacja portable-pty
    pub fn spawn(&self, work_dir: &std::path::Path, shell_cmd: &str) -> Result<()> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })?;
        let mut cmd = CommandBuilder::new(if cfg!(target_os = "windows") { "powershell" } else { "sh" });
        if cfg!(target_os = "windows") {
            cmd.args(["-NoProfile", "-Command", shell_cmd]);
        } else {
            cmd.args(["-c", shell_cmd]);
        }
        cmd.cwd(work_dir);
        let _child = pair.slave.spawn_command(cmd)?;
        let mut reader = pair.master.try_clone_reader()?;
        let buf = self.buffer.clone();
        std::thread::spawn(move || {
            let mut tmp = [0u8; 1024];
            loop {
                match reader.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        let s = String::from_utf8_lossy(&tmp[..n]).to_string();
                        if let Ok(mut b) = buf.lock() { b.push_str(&s); if b.len() > 8000 { let drain = b.len() - 8000; b.drain(..drain); } }
                    },
                    Err(_) => break,
                }
            }
        });
        // Writer kept for future input
        Ok(())
    }

    pub fn snapshot(&self) -> String {
        self.buffer.lock().map(|b| b.clone()).unwrap_or_default()
    }

    pub fn clear(&self) { if let Ok(mut b) = self.buffer.lock() { b.clear(); } }
}
