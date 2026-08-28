use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileItem {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_parent: bool,
    pub size_bytes: u64,
    pub modified_str: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileViewMode {
    List,
    Tree,
}

#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
    pub is_expanded: bool,
}

#[derive(Debug, Clone)]
pub struct PaneState {
    pub current_dir: PathBuf,
    pub items: Vec<FileItem>,
    pub selected_index: usize,
    pub view_mode: FileViewMode,
    pub tree_entries: Vec<TreeEntry>,
    // TC/DC: quick search + multi-select
    pub quick_search: String,
    pub filtered_indices: Option<Vec<usize>>, // None = brak filtra
    pub selected_paths: std::collections::HashSet<PathBuf>, // zaznaczenia Space/Ins
    pub drive_hotlist: Vec<PathBuf>, // Ulubione ścieżki jak Ctrl+D w TC
}

impl PaneState {
    pub fn new(dir: PathBuf, root_dir: &Path) -> Self {
        let mut pane = Self {
            current_dir: dir,
            items: Vec::new(),
            selected_index: 0,
            view_mode: FileViewMode::List,
            tree_entries: Vec::new(),
            quick_search: String::new(),
            filtered_indices: None,
            selected_paths: std::collections::HashSet::new(),
            drive_hotlist: vec![root_dir.to_path_buf()],
        };
        pane.refresh(root_dir).ok();
        pane.build_tree(root_dir);
        pane
    }

    pub fn toggle_view_mode(&mut self, root_dir: &Path) {
        self.view_mode = match self.view_mode {
            FileViewMode::List => {
                self.build_tree(root_dir);
                FileViewMode::Tree
            }
            FileViewMode::Tree => FileViewMode::List,
        };
        self.selected_index = 0;
    }

    pub fn build_tree(&mut self, _root_dir: &Path) {
        let mut entries = Vec::new();
        build_tree_recursive(&self.current_dir, 0, &mut entries, 3);
        self.tree_entries = entries;
    }

    // TC: kolor wg rozszerzenia (jak w Total Commander - kolory plików)
    pub fn file_color(name: &str, is_dir: bool) -> &'static str {
        if is_dir { return "dir"; }
        let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
        match ext.as_str() {
            "rs" | "go" | "py" | "js" | "ts" | "tsx" | "java" | "cpp" | "c" | "h" => "code",
            "json" | "toml" | "yaml" | "yml" | "xml" => "config",
            "md" | "txt" | "log" => "text",
            "zip" | "tar" | "gz" | "7z" | "rar" => "archive",
            "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => "image",
            "exe" | "dll" | "so" | "bin" => "binary",
            _ => "default",
        }
    }

    // Quick search: filtruj po wpisaniu (jak TC incremental search)
    pub fn apply_quick_search(&mut self, query: &str) {
        self.quick_search = query.to_string();
        if query.is_empty() {
            self.filtered_indices = None;
        } else {
            let q = query.to_lowercase();
            let idx: Vec<usize> = self.items.iter().enumerate()
                .filter(|(_, it)| it.name.to_lowercase().contains(&q))
                .map(|(i, _)| i).collect();
            self.filtered_indices = Some(idx);
        }
        self.selected_index = 0;
    }

    pub fn clear_quick_search(&mut self) {
        self.quick_search.clear();
        self.filtered_indices = None;
        self.selected_index = 0;
    }

    pub fn visible_items(&self) -> Vec<(usize, &FileItem)> {
        if let Some(ref idxs) = self.filtered_indices {
            idxs.iter().filter_map(|&i| self.items.get(i).map(|it| (i, it))).collect()
        } else {
            self.items.iter().enumerate().collect()
        }
    }

    pub fn toggle_select_current(&mut self) {
        if let Some(item) = self.items.get(self.selected_index) {
            if item.is_parent { return; }
            if !self.selected_paths.remove(&item.path) {
                self.selected_paths.insert(item.path.clone());
            }
        }
        // TC: po Space/Ins przeskocz na następny
        self.navigate_down();
    }

    pub fn selection_count(&self) -> usize { self.selected_paths.len() }

    pub fn selected_or_current_paths(&self) -> Vec<PathBuf> {
        if self.selected_paths.is_empty() {
            self.get_selected_item().map(|it| vec![it.path.clone()]).unwrap_or_default()
        } else {
            self.selected_paths.iter().cloned().collect()
        }
    }

    pub fn refresh(&mut self, root_dir: &Path) -> Result<()> {
        let mut items = Vec::new();

        // Jeśli nie jesteśmy w katalogu głównym projektu, dodaj ".." do wyjścia wyżej
        if self.current_dir != root_dir {
            if let Some(parent) = self.current_dir.parent() {
                items.push(FileItem {
                    name: ".. [katalog nadrzędny]".to_string(),
                    path: parent.to_path_buf(),
                    is_dir: true,
                    is_parent: true,
                    size_bytes: 0,
                    modified_str: String::new(),
                });
            }
        }

        let mut dirs = Vec::new();
        let mut files = Vec::new();

        if self.current_dir.exists() {
            for entry in fs::read_dir(&self.current_dir)? {
                let entry = entry?;
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if name == ".git" || name == "target" || name == "node_modules" {
                    continue;
                }

                let meta = entry.metadata().ok();
                let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
                let size = meta.as_ref().map_or(0, |m| m.len());

                let modified_str = meta
                    .and_then(|m| m.modified().ok())
                    .map(|st| {
                        let dt: chrono::DateTime<chrono::Utc> = st.into();
                        dt.format("%Y-%m-%d %H:%M").to_string()
                    })
                    .unwrap_or_default();

                let item = FileItem {
                    name,
                    path,
                    is_dir,
                    is_parent: false,
                    size_bytes: size,
                    modified_str,
                };

                if is_dir {
                    dirs.push(item);
                } else {
                    files.push(item);
                }
            }
        }

        dirs.sort_by_key(|a| a.name.to_lowercase());
        files.sort_by_key(|a| a.name.to_lowercase());

        items.extend(dirs);
        items.extend(files);

        self.items = items;
        if self.selected_index >= self.items.len() && !self.items.is_empty() {
            self.selected_index = self.items.len() - 1;
        }

        self.build_tree(root_dir);

        Ok(())
    }

    pub fn navigate_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn navigate_down(&mut self) {
        let max = if self.view_mode == FileViewMode::Tree {
            self.tree_entries.len()
        } else {
            self.items.len()
        };
        if max > 0 && self.selected_index + 1 < max {
            self.selected_index += 1;
        }
    }

    pub fn get_selected_item(&self) -> Option<&FileItem> {
        self.items.get(self.selected_index)
    }

    pub fn enter_selected(&mut self, root_dir: &Path) -> Option<PathBuf> {
        if let Some(item) = self.items.get(self.selected_index) {
            if item.is_dir {
                self.current_dir = item.path.clone();
                self.selected_index = 0;
                self.refresh(root_dir).ok();
                None
            } else {
                Some(item.path.clone())
            }
        } else {
            None
        }
    }

    pub fn go_to_parent(&mut self, root_dir: &Path) {
        if self.current_dir != root_dir {
            if let Some(parent) = self.current_dir.parent() {
                self.current_dir = parent.to_path_buf();
                self.selected_index = 0;
                self.refresh(root_dir).ok();
            }
        }
    }
}

pub struct FileManagerState {
    pub root_dir: PathBuf,
    pub active_pane: ActivePane,
    pub left: PaneState,
    pub right: PaneState,
}

impl FileManagerState {
    pub fn new(root_dir: PathBuf) -> Self {
        let left = PaneState::new(root_dir.clone(), &root_dir);
        let right = PaneState::new(root_dir.clone(), &root_dir);

        Self {
            root_dir,
            active_pane: ActivePane::Left,
            left,
            right,
        }
    }

    pub fn switch_pane(&mut self) {
        self.active_pane = match self.active_pane {
            ActivePane::Left => ActivePane::Right,
            ActivePane::Right => ActivePane::Left,
        };
    }

    pub fn active_mut(&mut self) -> &mut PaneState {
        match self.active_pane {
            ActivePane::Left => &mut self.left,
            ActivePane::Right => &mut self.right,
        }
    }

    pub fn active(&self) -> &PaneState {
        match self.active_pane {
            ActivePane::Left => &self.left,
            ActivePane::Right => &self.right,
        }
    }

    pub fn inactive(&self) -> &PaneState {
        match self.active_pane {
            ActivePane::Left => &self.right,
            ActivePane::Right => &self.left,
        }
    }

    pub fn refresh_all(&mut self) {
        let root = self.root_dir.clone();
        self.left.refresh(&root).ok();
        self.right.refresh(&root).ok();
    }

    pub fn get_relative_path(&self, item: &FileItem) -> String {
        if let Ok(rel) = item.path.strip_prefix(&self.root_dir) {
            rel.to_string_lossy().replace('\\', "/")
        } else {
            item.name.clone()
        }
    }

    /// TC: batch copy dla multi-select (Space/Ins)
    /// Kopiuje zaznaczony plik/katalog z aktywnego panelu do katalogu w nieaktywnym panelu
    pub fn copy_to_other_pane(&mut self) -> Result<String> {
        let target_dir = self.inactive().current_dir.clone();
        let paths = self.active().selected_or_current_paths();
        if paths.is_empty() { return Err(anyhow!("Brak zaznaczonego pliku")); }
        if paths.len() > 1 {
            let mut ok = 0;
            for p in &paths {
                let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                let dest = target_dir.join(&name);
                if p.is_dir() { let _ = copy_dir_all(p, &dest); } else { let _ = fs::copy(p, &dest); }
                ok += 1;
            }
            self.active_mut().selected_paths.clear();
            self.refresh_all();
            return Ok(format!("Skopiowano {} elementów do: {}", ok, target_dir.display()));
        }
        if let Some(item) = self.active().get_selected_item() {
            if item.is_parent {
                return Err(anyhow!("Nie można skopiować katalogu nadrzędnego"));
            }
            let dest_path = target_dir.join(&item.name);
            if item.is_dir {
                copy_dir_all(&item.path, &dest_path)?;
                self.refresh_all();
                return Ok(format!("Skopiowano katalog do: {}", dest_path.display()));
            } else {
                fs::copy(&item.path, &dest_path)?;
                self.refresh_all();
                return Ok(format!("Skopiowano plik do: {}", dest_path.display()));
            }
        }
        Err(anyhow!("Brak zaznaczonego pliku"))
    }

    /// Przenosi zaznaczony plik/katalog z aktywnego panelu do drugiego panelu
    pub fn move_to_other_pane(&mut self) -> Result<String> {
        let target_dir = self.inactive().current_dir.clone();

        if let Some(item) = self.active().get_selected_item() {
            if item.is_parent {
                return Err(anyhow!("Nie można przenieść katalogu nadrzędnego"));
            }
            let dest_path = target_dir.join(&item.name);
            fs::rename(&item.path, &dest_path)?;
            self.refresh_all();
            return Ok(format!("Przeniesiono do: {}", dest_path.display()));
        }
        Err(anyhow!("Brak zaznaczonego pliku"))
    }

    /// Usuwa zaznaczony plik/katalog w aktywnym panelu (batch jeśli multi-select)
    pub fn delete_selected(&mut self) -> Result<String> {
        let root = self.root_dir.clone();
        let paths = self.active().selected_or_current_paths();
        if paths.len() > 1 {
            let mut ok = 0;
            for p in &paths {
                if p.is_dir() { let _ = fs::remove_dir_all(p); } else { let _ = fs::remove_file(p); }
                ok += 1;
            }
            self.active_mut().selected_paths.clear();
            self.active_mut().refresh(&root).ok();
            return Ok(format!("Usunięto {} elementów", ok));
        }
        if let Some(item) = self.active().get_selected_item() {
            if item.is_parent {
                return Err(anyhow!("Nie można usunąć katalogu nadrzędnego"));
            }
            let name = item.name.clone();
            if item.is_dir {
                fs::remove_dir_all(&item.path)?;
            } else {
                fs::remove_file(&item.path)?;
            }
            self.active_mut().refresh(&root).ok();
            return Ok(format!("Usunięto: {}", name));
        }
        Err(anyhow!("Brak zaznaczonego elementu"))
    }

    // TC: Multi-rename (Ctrl+M) - wzorzec `*` -> nazwa
    pub fn multi_rename(&mut self, pattern: &str) -> Result<String> {
        let paths = self.active().selected_or_current_paths();
        if paths.is_empty() { return Err(anyhow!("Brak plików do przemianowania")); }
        let mut ok = 0;
        for p in paths {
            if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                let new_name = if pattern.contains('*') { pattern.replace('*', fname) } else { format!("{}_{}", pattern, fname) };
                let dest = p.parent().unwrap_or(&self.active().current_dir).join(new_name);
                let _ = fs::rename(&p, &dest);
                ok += 1;
            }
        }
        self.active_mut().selected_paths.clear();
        self.refresh_all();
        Ok(format!("Przemianowano {} elementów wg wzorca `{}`", ok, pattern))
    }

    // WCX: ZIP jako katalog — listuj zawartość bez wypakowania (podgląd)
    pub fn zip_preview(path: &Path) -> Result<Vec<String>> {
        let data = fs::read(path)?;
        // Prosty parser nagłówków ZIP (bez zewnętrznej lib) — szukamy nazw plików w central directory
        let mut names = Vec::new();
        let mut i = 0;
        while i + 30 < data.len() {
            if data[i]==0x50 && data[i+1]==0x4b && data[i+2]==0x03 && data[i+3]==0x04
                && i+26 < data.len() {
                    let name_len = u16::from_le_bytes([data[i+26], data[i+27]]) as usize;
                    if i+30+name_len <= data.len() {
                        if let Ok(n) = String::from_utf8(data[i+30..i+30+name_len].to_vec()) {
                            names.push(n);
                        }
                    }
                }
            i += 1;
            if names.len() > 100 { break; }
        }
        if names.is_empty() { names.push("(nie udało się odczytać nagłówków ZIP — użyj podglądu F3)".to_string()); }
        Ok(names)
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn build_tree_recursive(dir: &Path, depth: usize, out: &mut Vec<TreeEntry>, max_depth: usize) {
    if depth > max_depth || !dir.exists() {
        return;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if name == ".git" || name == "target" || name == "node_modules" {
                continue;
            }

            let is_dir = entry.metadata().is_ok_and(|m| m.is_dir());

            if is_dir {
                dirs.push((name, path));
            } else {
                files.push((name, path));
            }
        }

        dirs.sort_by_key(|a| a.0.to_lowercase());
        files.sort_by_key(|a| a.0.to_lowercase());

        for (name, path) in dirs {
            out.push(TreeEntry {
                name,
                path: path.clone(),
                is_dir: true,
                depth,
                is_expanded: true,
            });
            build_tree_recursive(&path, depth + 1, out, max_depth);
        }

        for (name, path) in files {
            out.push(TreeEntry {
                name,
                path,
                is_dir: false,
                depth,
                is_expanded: false,
            });
        }
    }
}
