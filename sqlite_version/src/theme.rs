use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub enum ThemeMode {
    #[default]
    OpenCodeDark,      // Nowoczesny, domyślny styl oryginalnego OpenCode
    GraphiteGrey,      // Mocno szary, profesjonalny grafit (Zinc/Slate)
    OledPureBlack,     // 100% czyste czernie dla ekranów OLED
    CatppuccinMocha,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    TokyoNight,
    Cyberpunk,
    Dracula,
    Nord,
    Gruvbox,
    Monokai,
    ClassicBlue,
    // Pełna lista z opencode packages/ui/src/theme/default-themes.ts (38)
    Oc2,
    Amoled,
    Aura,
    Ayu,
    Carbonfox,
    Cobalt2,
    Cursor,
    Everforest,
    Flexoki,
    Github,
    Kanagawa,
    LucentOrng,
    Material,
    Matrix,
    Mercury,
    Nightowl,
    OneDark,
    OneDarkPro,
    Orng,
    OsakaJade,
    Palenight,
    Rosepine,
    ShadesOfPurple,
    Solarized,
    Synthwave84,
    Vercel,
    Vesper,
    Zenburn,
    System, // adaptuje się do tła terminala
}


#[derive(Debug, Clone)]
pub struct AppTheme {
    pub name: &'static str,
    pub mode: ThemeMode,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub border: Color,
    pub border_active: Color,
    pub bg_card: Color,
    pub user: Color,
    pub assistant: Color,
    pub system: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub text_muted: Color,
}

impl AppTheme {
    pub fn get(mode: &ThemeMode) -> Self {
        match mode {
            ThemeMode::OpenCodeDark => Self {
                name: "OpenCode Dark (Modern)",
                mode: ThemeMode::OpenCodeDark,
                primary: Color::Rgb(99, 102, 241),      // Indigo 500
                secondary: Color::Rgb(168, 85, 247),    // Purple 500
                accent: Color::Rgb(56, 189, 248),       // Sky 400
                border: Color::Rgb(55, 65, 81),         // Gray 700
                border_active: Color::Rgb(129, 140, 248),// Indigo 400
                bg_card: Color::Rgb(31, 41, 55),        // Gray 800
                user: Color::Rgb(96, 165, 250),         // Blue 400
                assistant: Color::Rgb(52, 211, 153),    // Emerald 400
                system: Color::Rgb(251, 191, 36),       // Amber 400
                success: Color::Rgb(52, 211, 153),      // Emerald 400
                warning: Color::Rgb(251, 146, 60),      // Orange 400
                error: Color::Rgb(248, 113, 113),       // Red 400
                text_muted: Color::Rgb(156, 163, 175),  // Gray 400
            },
            ThemeMode::GraphiteGrey => Self {
                name: "Graphite Grey (Matowy Szary)",
                mode: ThemeMode::GraphiteGrey,
                primary: Color::Rgb(212, 212, 216),     // Zinc 300
                secondary: Color::Rgb(161, 161, 170),   // Zinc 400
                accent: Color::Rgb(228, 228, 231),      // Zinc 200
                border: Color::Rgb(63, 63, 70),         // Zinc 700
                border_active: Color::Rgb(161, 161, 170),// Zinc 400
                bg_card: Color::Rgb(39, 39, 42),        // Zinc 800
                user: Color::Rgb(244, 244, 245),        // Zinc 100
                assistant: Color::Rgb(161, 161, 170),   // Zinc 400
                system: Color::Rgb(212, 212, 216),      // Zinc 300
                success: Color::Rgb(161, 161, 170),
                warning: Color::Rgb(212, 212, 216),
                error: Color::Rgb(248, 113, 113),
                text_muted: Color::Rgb(113, 113, 122),  // Zinc 500
            },
            ThemeMode::OledPureBlack => Self {
                name: "OLED Pure Black (Zero-Burn Protection)",
                mode: ThemeMode::OledPureBlack,
                primary: Color::Rgb(255, 255, 255),     // Czysta Biel
                secondary: Color::Rgb(180, 180, 180),
                accent: Color::Rgb(120, 180, 255),
                border: Color::Rgb(40, 40, 40),         // Ciemnoszary obrys (piksele wygaszone w 95%)
                border_active: Color::Rgb(100, 100, 100),
                bg_card: Color::Rgb(0, 0, 0),           // 100% Czerń #000000
                user: Color::Rgb(255, 255, 255),
                assistant: Color::Rgb(200, 200, 200),
                system: Color::Rgb(160, 160, 160),
                success: Color::Rgb(180, 255, 180),
                warning: Color::Rgb(255, 220, 150),
                error: Color::Rgb(255, 120, 120),
                text_muted: Color::Rgb(90, 90, 90),
            },
            ThemeMode::CatppuccinMocha => Self {
                name: "Catppuccin Mocha",
                mode: ThemeMode::CatppuccinMocha,
                primary: Color::Rgb(203, 166, 247),     // Mauve
                secondary: Color::Rgb(137, 180, 250),   // Blue
                accent: Color::Rgb(148, 226, 213),      // Teal
                border: Color::Rgb(69, 71, 90),         // Surface1
                border_active: Color::Rgb(203, 166, 247),
                bg_card: Color::Rgb(30, 30, 46),        // Base
                user: Color::Rgb(137, 180, 250),
                assistant: Color::Rgb(166, 227, 161),   // Green
                system: Color::Rgb(249, 226, 175),      // Yellow
                success: Color::Rgb(166, 227, 161),
                warning: Color::Rgb(250, 179, 135),     // Peach
                error: Color::Rgb(243, 139, 168),       // Red
                text_muted: Color::Rgb(147, 153, 178),
            },
            ThemeMode::CatppuccinFrappe => Self {
                name: "Catppuccin Frappe",
                mode: ThemeMode::CatppuccinFrappe,
                primary: Color::Rgb(202, 158, 230),     // Mauve Frappe
                secondary: Color::Rgb(133, 193, 233),   // Blue Frappe
                accent: Color::Rgb(129, 200, 190),      // Teal Frappe
                border: Color::Rgb(65, 69, 89),         // Surface1 Frappe
                border_active: Color::Rgb(202, 158, 230),
                bg_card: Color::Rgb(48, 52, 70),        // Base Frappe
                user: Color::Rgb(133, 193, 233),
                assistant: Color::Rgb(166, 209, 137),
                system: Color::Rgb(238, 212, 159),
                success: Color::Rgb(166, 209, 137),
                warning: Color::Rgb(238, 190, 130),
                error: Color::Rgb(231, 130, 132),
                text_muted: Color::Rgb(148, 156, 187),
            },
            ThemeMode::CatppuccinMacchiato => Self {
                name: "Catppuccin Macchiato",
                mode: ThemeMode::CatppuccinMacchiato,
                primary: Color::Rgb(202, 158, 230),     // Mauve Macchiato
                secondary: Color::Rgb(138, 173, 244),   // Blue Macchiato
                accent: Color::Rgb(139, 213, 202),      // Teal Macchiato
                border: Color::Rgb(54, 58, 79),         // Surface1 Macchiato
                border_active: Color::Rgb(202, 158, 230),
                bg_card: Color::Rgb(36, 39, 58),        // Base Macchiato
                user: Color::Rgb(138, 173, 244),
                assistant: Color::Rgb(166, 218, 149),
                system: Color::Rgb(238, 212, 159),
                success: Color::Rgb(166, 218, 149),
                warning: Color::Rgb(245, 169, 127),
                error: Color::Rgb(237, 135, 150),
                text_muted: Color::Rgb(147, 154, 183),
            },
            ThemeMode::TokyoNight => Self {
                name: "Tokyo Night",
                mode: ThemeMode::TokyoNight,
                primary: Color::Rgb(122, 162, 247),
                secondary: Color::Rgb(187, 154, 247),
                accent: Color::Rgb(125, 207, 255),
                border: Color::Rgb(41, 46, 66),
                border_active: Color::Rgb(122, 162, 247),
                bg_card: Color::Rgb(26, 27, 38),
                user: Color::Rgb(122, 162, 247),
                assistant: Color::Rgb(158, 206, 106),
                system: Color::Rgb(224, 175, 104),
                success: Color::Rgb(158, 206, 106),
                warning: Color::Rgb(255, 158, 100),
                error: Color::Rgb(247, 118, 142),
                text_muted: Color::Rgb(115, 122, 162),
            },
            ThemeMode::Dracula => Self {
                name: "Dracula",
                mode: ThemeMode::Dracula,
                primary: Color::Rgb(189, 147, 249),
                secondary: Color::Rgb(255, 121, 198),
                accent: Color::Rgb(139, 233, 253),
                border: Color::Rgb(68, 71, 90),
                border_active: Color::Rgb(189, 147, 249),
                bg_card: Color::Rgb(40, 42, 54),
                user: Color::Rgb(139, 233, 253),
                assistant: Color::Rgb(80, 250, 123),
                system: Color::Rgb(241, 250, 140),
                success: Color::Rgb(80, 250, 123),
                warning: Color::Rgb(255, 184, 108),
                error: Color::Rgb(255, 85, 85),
                text_muted: Color::Rgb(98, 114, 164),
            },
            ThemeMode::Nord => Self {
                name: "Nord Frost",
                mode: ThemeMode::Nord,
                primary: Color::Rgb(136, 192, 208),
                secondary: Color::Rgb(129, 161, 193),
                accent: Color::Rgb(94, 129, 172),
                border: Color::Rgb(59, 66, 82),
                border_active: Color::Rgb(136, 192, 208),
                bg_card: Color::Rgb(46, 52, 64),
                user: Color::Rgb(236, 239, 244),
                assistant: Color::Rgb(163, 190, 140),
                system: Color::Rgb(235, 203, 139),
                success: Color::Rgb(163, 190, 140),
                warning: Color::Rgb(208, 135, 112),
                error: Color::Rgb(191, 97, 106),
                text_muted: Color::Rgb(76, 86, 106),
            },
            ThemeMode::Cyberpunk => Self {
                name: "Cyberpunk Neon",
                mode: ThemeMode::Cyberpunk,
                primary: Color::Rgb(0, 255, 234),
                secondary: Color::Rgb(255, 0, 128),
                accent: Color::Rgb(255, 230, 0),
                border: Color::Rgb(50, 50, 80),
                border_active: Color::Rgb(0, 255, 234),
                bg_card: Color::Rgb(20, 20, 35),
                user: Color::Rgb(0, 255, 234),
                assistant: Color::Rgb(0, 255, 128),
                system: Color::Rgb(255, 230, 0),
                success: Color::Rgb(0, 255, 128),
                warning: Color::Rgb(255, 170, 0),
                error: Color::Rgb(255, 50, 80),
                text_muted: Color::Rgb(120, 120, 160),
            },
            ThemeMode::Gruvbox => Self {
                name: "Gruvbox Dark",
                mode: ThemeMode::Gruvbox,
                primary: Color::Rgb(250, 189, 47),
                secondary: Color::Rgb(254, 128, 25),
                accent: Color::Rgb(131, 165, 152),
                border: Color::Rgb(80, 73, 69),
                border_active: Color::Rgb(250, 189, 47),
                bg_card: Color::Rgb(40, 40, 40),
                user: Color::Rgb(235, 219, 178),
                assistant: Color::Rgb(184, 187, 38),
                system: Color::Rgb(250, 189, 47),
                success: Color::Rgb(184, 187, 38),
                warning: Color::Rgb(254, 128, 25),
                error: Color::Rgb(251, 73, 52),
                text_muted: Color::Rgb(168, 153, 132),
            },
            ThemeMode::Monokai => Self {
                name: "Monokai Pro",
                mode: ThemeMode::Monokai,
                primary: Color::Rgb(255, 216, 102),
                secondary: Color::Rgb(255, 97, 136),
                accent: Color::Rgb(120, 220, 232),
                border: Color::Rgb(64, 62, 67),
                border_active: Color::Rgb(255, 216, 102),
                bg_card: Color::Rgb(45, 42, 46),
                user: Color::Rgb(255, 216, 102),
                assistant: Color::Rgb(169, 220, 107),
                system: Color::Rgb(255, 97, 136),
                success: Color::Rgb(169, 220, 107),
                warning: Color::Rgb(252, 152, 103),
                error: Color::Rgb(255, 97, 136),
                text_muted: Color::Rgb(114, 112, 114),
            },
            ThemeMode::ClassicBlue => Self {
                name: "Classic Blue",
                mode: ThemeMode::ClassicBlue,
                primary: Color::Rgb(100, 180, 255),
                secondary: Color::Rgb(255, 220, 100),
                accent: Color::Rgb(100, 255, 200),
                border: Color::Rgb(40, 80, 140),
                border_active: Color::Rgb(100, 180, 255),
                bg_card: Color::Rgb(15, 30, 60),
                user: Color::Rgb(255, 255, 255),
                assistant: Color::Rgb(100, 255, 200),
                system: Color::Rgb(255, 220, 100),
                success: Color::Rgb(100, 255, 150),
                warning: Color::Rgb(255, 200, 100),
                error: Color::Rgb(255, 100, 100),
                text_muted: Color::Rgb(140, 160, 200),
            },
            // ── Import z opencode default-themes.ts (38) ──
            ThemeMode::Oc2 => Self { name: "OC-2", mode: ThemeMode::Oc2, primary: Color::Rgb(120, 120, 255), secondary: Color::Rgb(180, 120, 255), accent: Color::Rgb(100, 220, 255), border: Color::Rgb(50, 50, 70), border_active: Color::Rgb(120, 120, 255), bg_card: Color::Rgb(18, 18, 28), user: Color::Rgb(120, 180, 255), assistant: Color::Rgb(120, 255, 180), system: Color::Rgb(255, 220, 120), success: Color::Rgb(120, 255, 180), warning: Color::Rgb(255, 200, 100), error: Color::Rgb(255, 120, 120), text_muted: Color::Rgb(130, 130, 150) },
            ThemeMode::Amoled => Self { name: "AMOLED", mode: ThemeMode::Amoled, primary: Color::Rgb(255, 255, 255), secondary: Color::Rgb(180, 180, 180), accent: Color::Rgb(0, 255, 150), border: Color::Rgb(30, 30, 30), border_active: Color::Rgb(255, 255, 255), bg_card: Color::Rgb(0, 0, 0), user: Color::Rgb(255, 255, 255), assistant: Color::Rgb(0, 255, 150), system: Color::Rgb(255, 255, 0), success: Color::Rgb(0, 255, 150), warning: Color::Rgb(255, 200, 0), error: Color::Rgb(255, 80, 80), text_muted: Color::Rgb(100, 100, 100) },
            ThemeMode::Aura => Self { name: "Aura", mode: ThemeMode::Aura, primary: Color::Rgb(180, 130, 255), secondary: Color::Rgb(130, 180, 255), accent: Color::Rgb(255, 130, 180), border: Color::Rgb(45, 45, 65), border_active: Color::Rgb(180, 130, 255), bg_card: Color::Rgb(28, 28, 42), user: Color::Rgb(180, 130, 255), assistant: Color::Rgb(130, 255, 180), system: Color::Rgb(255, 210, 120), success: Color::Rgb(130, 255, 180), warning: Color::Rgb(255, 180, 100), error: Color::Rgb(255, 100, 130), text_muted: Color::Rgb(140, 140, 170) },
            ThemeMode::Ayu => Self { name: "Ayu Dark", mode: ThemeMode::Ayu, primary: Color::Rgb(255, 204, 102), secondary: Color::Rgb(112, 192, 255), accent: Color::Rgb(151, 232, 168), border: Color::Rgb(45, 55, 65), border_active: Color::Rgb(255, 204, 102), bg_card: Color::Rgb(15, 20, 25), user: Color::Rgb(112, 192, 255), assistant: Color::Rgb(151, 232, 168), system: Color::Rgb(255, 204, 102), success: Color::Rgb(151, 232, 168), warning: Color::Rgb(255, 180, 80), error: Color::Rgb(255, 100, 100), text_muted: Color::Rgb(120, 130, 140) },
            ThemeMode::Carbonfox => Self { name: "Carbonfox", mode: ThemeMode::Carbonfox, primary: Color::Rgb(110, 180, 180), secondary: Color::Rgb(200, 150, 110), accent: Color::Rgb(150, 180, 220), border: Color::Rgb(50, 60, 60), border_active: Color::Rgb(110, 180, 180), bg_card: Color::Rgb(22, 28, 32), user: Color::Rgb(150, 200, 200), assistant: Color::Rgb(180, 200, 150), system: Color::Rgb(220, 180, 120), success: Color::Rgb(150, 200, 150), warning: Color::Rgb(220, 180, 100), error: Color::Rgb(220, 100, 100), text_muted: Color::Rgb(110, 120, 130) },
            ThemeMode::Cobalt2 => Self { name: "Cobalt2", mode: ThemeMode::Cobalt2, primary: Color::Rgb(0, 140, 255), secondary: Color::Rgb(255, 180, 0), accent: Color::Rgb(0, 255, 200), border: Color::Rgb(25, 55, 95), border_active: Color::Rgb(0, 140, 255), bg_card: Color::Rgb(0, 30, 60), user: Color::Rgb(100, 180, 255), assistant: Color::Rgb(100, 255, 180), system: Color::Rgb(255, 220, 100), success: Color::Rgb(100, 255, 150), warning: Color::Rgb(255, 200, 50), error: Color::Rgb(255, 80, 80), text_muted: Color::Rgb(80, 120, 180) },
            ThemeMode::Cursor => Self { name: "Cursor", mode: ThemeMode::Cursor, primary: Color::Rgb(80, 130, 255), secondary: Color::Rgb(150, 150, 150), accent: Color::Rgb(100, 200, 255), border: Color::Rgb(40, 40, 45), border_active: Color::Rgb(80, 130, 255), bg_card: Color::Rgb(22, 22, 24), user: Color::Rgb(120, 160, 255), assistant: Color::Rgb(150, 255, 150), system: Color::Rgb(255, 220, 120), success: Color::Rgb(150, 255, 150), warning: Color::Rgb(255, 200, 100), error: Color::Rgb(255, 100, 100), text_muted: Color::Rgb(130, 130, 140) },
            ThemeMode::Everforest => Self { name: "Everforest", mode: ThemeMode::Everforest, primary: Color::Rgb(167, 192, 128), secondary: Color::Rgb(214, 180, 120), accent: Color::Rgb(127, 180, 180), border: Color::Rgb(55, 65, 55), border_active: Color::Rgb(167, 192, 128), bg_card: Color::Rgb(40, 45, 40), user: Color::Rgb(167, 192, 128), assistant: Color::Rgb(140, 180, 140), system: Color::Rgb(214, 190, 120), success: Color::Rgb(140, 180, 120), warning: Color::Rgb(220, 180, 100), error: Color::Rgb(211, 112, 112), text_muted: Color::Rgb(120, 130, 120) },
            ThemeMode::Flexoki => Self { name: "Flexoki", mode: ThemeMode::Flexoki, primary: Color::Rgb(200, 120, 80), secondary: Color::Rgb(80, 150, 150), accent: Color::Rgb(180, 180, 80), border: Color::Rgb(60, 50, 45), border_active: Color::Rgb(200, 120, 80), bg_card: Color::Rgb(28, 24, 22), user: Color::Rgb(220, 140, 100), assistant: Color::Rgb(120, 180, 120), system: Color::Rgb(200, 180, 100), success: Color::Rgb(120, 180, 120), warning: Color::Rgb(220, 160, 80), error: Color::Rgb(220, 90, 90), text_muted: Color::Rgb(140, 130, 120) },
            ThemeMode::Github => Self { name: "GitHub Dark", mode: ThemeMode::Github, primary: Color::Rgb(88, 166, 255), secondary: Color::Rgb(139, 148, 158), accent: Color::Rgb(63, 185, 80), border: Color::Rgb(48, 54, 61), border_active: Color::Rgb(88, 166, 255), bg_card: Color::Rgb(13, 17, 23), user: Color::Rgb(88, 166, 255), assistant: Color::Rgb(63, 185, 80), system: Color::Rgb(210, 153, 34), success: Color::Rgb(63, 185, 80), warning: Color::Rgb(210, 153, 34), error: Color::Rgb(248, 81, 73), text_muted: Color::Rgb(139, 148, 158) },
            ThemeMode::Kanagawa => Self { name: "Kanagawa", mode: ThemeMode::Kanagawa, primary: Color::Rgb(122, 162, 247), secondary: Color::Rgb(220, 180, 120), accent: Color::Rgb(150, 180, 150), border: Color::Rgb(45, 45, 60), border_active: Color::Rgb(122, 162, 247), bg_card: Color::Rgb(30, 30, 46), user: Color::Rgb(122, 162, 247), assistant: Color::Rgb(150, 180, 150), system: Color::Rgb(220, 200, 120), success: Color::Rgb(150, 180, 120), warning: Color::Rgb(220, 180, 100), error: Color::Rgb(200, 100, 100), text_muted: Color::Rgb(120, 120, 140) },
            ThemeMode::LucentOrng => Self { name: "Lucent Orng", mode: ThemeMode::LucentOrng, primary: Color::Rgb(255, 140, 50), secondary: Color::Rgb(255, 200, 80), accent: Color::Rgb(255, 220, 120), border: Color::Rgb(60, 45, 30), border_active: Color::Rgb(255, 140, 50), bg_card: Color::Rgb(30, 25, 20), user: Color::Rgb(255, 160, 80), assistant: Color::Rgb(255, 200, 120), system: Color::Rgb(255, 220, 150), success: Color::Rgb(180, 220, 120), warning: Color::Rgb(255, 180, 60), error: Color::Rgb(255, 100, 80), text_muted: Color::Rgb(140, 120, 100) },
            ThemeMode::Material => Self { name: "Material", mode: ThemeMode::Material, primary: Color::Rgb(130, 170, 255), secondary: Color::Rgb(195, 132, 255), accent: Color::Rgb(100, 220, 220), border: Color::Rgb(45, 55, 70), border_active: Color::Rgb(130, 170, 255), bg_card: Color::Rgb(25, 30, 40), user: Color::Rgb(130, 170, 255), assistant: Color::Rgb(195, 232, 141), system: Color::Rgb(255, 203, 107), success: Color::Rgb(195, 232, 141), warning: Color::Rgb(255, 203, 107), error: Color::Rgb(255, 98, 140), text_muted: Color::Rgb(120, 130, 150) },
            ThemeMode::Matrix => Self { name: "Matrix", mode: ThemeMode::Matrix, primary: Color::Rgb(0, 255, 0), secondary: Color::Rgb(0, 200, 0), accent: Color::Rgb(100, 255, 100), border: Color::Rgb(0, 60, 0), border_active: Color::Rgb(0, 255, 0), bg_card: Color::Rgb(0, 15, 0), user: Color::Rgb(0, 255, 0), assistant: Color::Rgb(100, 255, 100), system: Color::Rgb(200, 255, 0), success: Color::Rgb(0, 255, 0), warning: Color::Rgb(200, 255, 0), error: Color::Rgb(255, 0, 0), text_muted: Color::Rgb(0, 150, 0) },
            ThemeMode::Mercury => Self { name: "Mercury", mode: ThemeMode::Mercury, primary: Color::Rgb(180, 180, 200), secondary: Color::Rgb(140, 140, 160), accent: Color::Rgb(200, 200, 220), border: Color::Rgb(60, 60, 70), border_active: Color::Rgb(180, 180, 200), bg_card: Color::Rgb(25, 25, 30), user: Color::Rgb(180, 180, 200), assistant: Color::Rgb(160, 160, 180), system: Color::Rgb(200, 200, 180), success: Color::Rgb(160, 200, 160), warning: Color::Rgb(200, 180, 140), error: Color::Rgb(200, 120, 120), text_muted: Color::Rgb(120, 120, 130) },
            ThemeMode::Nightowl => Self { name: "Night Owl", mode: ThemeMode::Nightowl, primary: Color::Rgb(0, 180, 255), secondary: Color::Rgb(200, 120, 255), accent: Color::Rgb(0, 255, 200), border: Color::Rgb(30, 45, 60), border_active: Color::Rgb(0, 180, 255), bg_card: Color::Rgb(1, 22, 39), user: Color::Rgb(100, 200, 255), assistant: Color::Rgb(100, 255, 200), system: Color::Rgb(255, 220, 100), success: Color::Rgb(100, 255, 150), warning: Color::Rgb(255, 200, 80), error: Color::Rgb(255, 80, 80), text_muted: Color::Rgb(80, 120, 160) },
            ThemeMode::OneDark => Self { name: "One Dark", mode: ThemeMode::OneDark, primary: Color::Rgb(97, 175, 239), secondary: Color::Rgb(198, 120, 221), accent: Color::Rgb(86, 182, 194), border: Color::Rgb(45, 53, 65), border_active: Color::Rgb(97, 175, 239), bg_card: Color::Rgb(40, 44, 52), user: Color::Rgb(97, 175, 239), assistant: Color::Rgb(152, 195, 121), system: Color::Rgb(229, 192, 123), success: Color::Rgb(152, 195, 121), warning: Color::Rgb(229, 192, 123), error: Color::Rgb(224, 108, 117), text_muted: Color::Rgb(120, 130, 145) },
            ThemeMode::OneDarkPro => Self { name: "One Dark Pro", mode: ThemeMode::OneDarkPro, primary: Color::Rgb(80, 160, 240), secondary: Color::Rgb(180, 100, 220), accent: Color::Rgb(70, 170, 180), border: Color::Rgb(40, 48, 60), border_active: Color::Rgb(80, 160, 240), bg_card: Color::Rgb(35, 40, 48), user: Color::Rgb(80, 160, 240), assistant: Color::Rgb(140, 180, 110), system: Color::Rgb(220, 180, 110), success: Color::Rgb(140, 180, 110), warning: Color::Rgb(220, 180, 100), error: Color::Rgb(210, 100, 110), text_muted: Color::Rgb(110, 120, 135) },
            ThemeMode::Orng => Self { name: "Orng", mode: ThemeMode::Orng, primary: Color::Rgb(255, 120, 40), secondary: Color::Rgb(255, 180, 60), accent: Color::Rgb(255, 220, 100), border: Color::Rgb(65, 45, 30), border_active: Color::Rgb(255, 120, 40), bg_card: Color::Rgb(32, 22, 15), user: Color::Rgb(255, 140, 60), assistant: Color::Rgb(255, 200, 100), system: Color::Rgb(255, 220, 150), success: Color::Rgb(180, 220, 100), warning: Color::Rgb(255, 180, 40), error: Color::Rgb(255, 80, 60), text_muted: Color::Rgb(140, 110, 90) },
            ThemeMode::OsakaJade => Self { name: "Osaka Jade", mode: ThemeMode::OsakaJade, primary: Color::Rgb(0, 180, 160), secondary: Color::Rgb(200, 180, 120), accent: Color::Rgb(100, 200, 180), border: Color::Rgb(35, 60, 55), border_active: Color::Rgb(0, 180, 160), bg_card: Color::Rgb(20, 32, 30), user: Color::Rgb(80, 200, 180), assistant: Color::Rgb(180, 200, 140), system: Color::Rgb(220, 200, 120), success: Color::Rgb(140, 200, 140), warning: Color::Rgb(220, 180, 100), error: Color::Rgb(200, 100, 100), text_muted: Color::Rgb(110, 130, 125) },
            ThemeMode::Palenight => Self { name: "Palenight", mode: ThemeMode::Palenight, primary: Color::Rgb(130, 170, 255), secondary: Color::Rgb(195, 132, 255), accent: Color::Rgb(100, 220, 200), border: Color::Rgb(45, 55, 75), border_active: Color::Rgb(130, 170, 255), bg_card: Color::Rgb(41, 45, 62), user: Color::Rgb(130, 170, 255), assistant: Color::Rgb(195, 232, 141), system: Color::Rgb(255, 203, 107), success: Color::Rgb(195, 232, 141), warning: Color::Rgb(255, 203, 107), error: Color::Rgb(255, 98, 140), text_muted: Color::Rgb(120, 130, 160) },
            ThemeMode::Rosepine => Self { name: "Rosé Pine", mode: ThemeMode::Rosepine, primary: Color::Rgb(235, 188, 186), secondary: Color::Rgb(196, 167, 231), accent: Color::Rgb(156, 207, 216), border: Color::Rgb(55, 50, 60), border_active: Color::Rgb(235, 188, 186), bg_card: Color::Rgb(35, 33, 42), user: Color::Rgb(235, 188, 186), assistant: Color::Rgb(156, 207, 216), system: Color::Rgb(246, 193, 119), success: Color::Rgb(156, 207, 160), warning: Color::Rgb(246, 193, 119), error: Color::Rgb(235, 111, 146), text_muted: Color::Rgb(144, 140, 170) },
            ThemeMode::ShadesOfPurple => Self { name: "Shades of Purple", mode: ThemeMode::ShadesOfPurple, primary: Color::Rgb(180, 120, 255), secondary: Color::Rgb(255, 120, 180), accent: Color::Rgb(120, 220, 255), border: Color::Rgb(50, 40, 70), border_active: Color::Rgb(180, 120, 255), bg_card: Color::Rgb(30, 25, 45), user: Color::Rgb(180, 120, 255), assistant: Color::Rgb(120, 255, 180), system: Color::Rgb(255, 220, 120), success: Color::Rgb(120, 255, 150), warning: Color::Rgb(255, 200, 100), error: Color::Rgb(255, 100, 130), text_muted: Color::Rgb(140, 120, 180) },
            ThemeMode::Solarized => Self { name: "Solarized Dark", mode: ThemeMode::Solarized, primary: Color::Rgb(38, 139, 210), secondary: Color::Rgb(211, 54, 130), accent: Color::Rgb(42, 161, 152), border: Color::Rgb(40, 50, 55), border_active: Color::Rgb(38, 139, 210), bg_card: Color::Rgb(0, 43, 54), user: Color::Rgb(38, 139, 210), assistant: Color::Rgb(133, 153, 0), system: Color::Rgb(181, 137, 0), success: Color::Rgb(133, 153, 0), warning: Color::Rgb(181, 137, 0), error: Color::Rgb(220, 50, 47), text_muted: Color::Rgb(101, 123, 131) },
            ThemeMode::Synthwave84 => Self { name: "Synthwave '84", mode: ThemeMode::Synthwave84, primary: Color::Rgb(255, 0, 128), secondary: Color::Rgb(0, 255, 255), accent: Color::Rgb(255, 240, 0), border: Color::Rgb(60, 30, 60), border_active: Color::Rgb(255, 0, 128), bg_card: Color::Rgb(30, 20, 40), user: Color::Rgb(255, 0, 128), assistant: Color::Rgb(0, 255, 200), system: Color::Rgb(255, 240, 0), success: Color::Rgb(0, 255, 150), warning: Color::Rgb(255, 200, 0), error: Color::Rgb(255, 60, 100), text_muted: Color::Rgb(160, 100, 180) },
            ThemeMode::Vercel => Self { name: "Vercel", mode: ThemeMode::Vercel, primary: Color::Rgb(255, 255, 255), secondary: Color::Rgb(150, 150, 150), accent: Color::Rgb(0, 112, 243), border: Color::Rgb(40, 40, 40), border_active: Color::Rgb(255, 255, 255), bg_card: Color::Rgb(10, 10, 10), user: Color::Rgb(255, 255, 255), assistant: Color::Rgb(0, 112, 243), system: Color::Rgb(255, 255, 255), success: Color::Rgb(0, 200, 80), warning: Color::Rgb(255, 200, 0), error: Color::Rgb(255, 50, 50), text_muted: Color::Rgb(120, 120, 120) },
            ThemeMode::Vesper => Self { name: "Vesper", mode: ThemeMode::Vesper, primary: Color::Rgb(180, 180, 190), secondary: Color::Rgb(140, 140, 150), accent: Color::Rgb(200, 200, 210), border: Color::Rgb(50, 50, 55), border_active: Color::Rgb(180, 180, 190), bg_card: Color::Rgb(20, 20, 22), user: Color::Rgb(180, 180, 190), assistant: Color::Rgb(160, 160, 170), system: Color::Rgb(200, 200, 190), success: Color::Rgb(160, 200, 160), warning: Color::Rgb(200, 180, 140), error: Color::Rgb(200, 120, 120), text_muted: Color::Rgb(110, 110, 120) },
            ThemeMode::Zenburn => Self { name: "Zenburn", mode: ThemeMode::Zenburn, primary: Color::Rgb(220, 200, 140), secondary: Color::Rgb(140, 180, 140), accent: Color::Rgb(140, 180, 200), border: Color::Rgb(60, 60, 55), border_active: Color::Rgb(220, 200, 140), bg_card: Color::Rgb(50, 50, 50), user: Color::Rgb(220, 200, 140), assistant: Color::Rgb(140, 180, 140), system: Color::Rgb(220, 180, 120), success: Color::Rgb(140, 180, 120), warning: Color::Rgb(220, 180, 100), error: Color::Rgb(200, 100, 100), text_muted: Color::Rgb(140, 130, 120) },
            ThemeMode::System => Self { name: "System (Terminal)", mode: ThemeMode::System, primary: Color::White, secondary: Color::Gray, accent: Color::Cyan, border: Color::DarkGray, border_active: Color::White, bg_card: Color::Black, user: Color::White, assistant: Color::Gray, system: Color::Yellow, success: Color::Green, warning: Color::Yellow, error: Color::Red, text_muted: Color::DarkGray },
        }
    }

    pub fn parse(s: &str) -> Option<ThemeMode> {
        match s.trim().to_lowercase().as_str() {
            "opencode" | "dark" | "modern" | "default" => Some(ThemeMode::OpenCodeDark),
            "graphite" | "grey" | "gray" | "szary" | "zinc" => Some(ThemeMode::GraphiteGrey),
            "oled" | "pureblack" | "black" | "czarny" => Some(ThemeMode::OledPureBlack),
            "catppuccin" | "catppuccin-mocha" | "mocha" => Some(ThemeMode::CatppuccinMocha),
            "catppuccin-frappe" | "frappe" => Some(ThemeMode::CatppuccinFrappe),
            "catppuccin-macchiato" | "macchiato" => Some(ThemeMode::CatppuccinMacchiato),
            "tokyo" | "tokyonight" | "tokyo-night" => Some(ThemeMode::TokyoNight),
            "dracula" => Some(ThemeMode::Dracula),
            "nord" => Some(ThemeMode::Nord),
            "cyberpunk" => Some(ThemeMode::Cyberpunk),
            "gruvbox" => Some(ThemeMode::Gruvbox),
            "monokai" => Some(ThemeMode::Monokai),
            "classic" | "blue" | "classicblue" => Some(ThemeMode::ClassicBlue),
            "oc-2" | "oc2" => Some(ThemeMode::Oc2),
            "amoled" => Some(ThemeMode::Amoled),
            "aura" => Some(ThemeMode::Aura),
            "ayu" => Some(ThemeMode::Ayu),
            "carbonfox" | "carbon" => Some(ThemeMode::Carbonfox),
            "cobalt2" | "cobalt" => Some(ThemeMode::Cobalt2),
            "cursor" => Some(ThemeMode::Cursor),
            "everforest" => Some(ThemeMode::Everforest),
            "flexoki" => Some(ThemeMode::Flexoki),
            "github" => Some(ThemeMode::Github),
            "kanagawa" => Some(ThemeMode::Kanagawa),
            "lucent-orng" | "lucent" => Some(ThemeMode::LucentOrng),
            "material" => Some(ThemeMode::Material),
            "matrix" => Some(ThemeMode::Matrix),
            "mercury" => Some(ThemeMode::Mercury),
            "nightowl" | "night-owl" => Some(ThemeMode::Nightowl),
            "one-dark" | "onedark" => Some(ThemeMode::OneDark),
            "onedarkpro" | "one-dark-pro" => Some(ThemeMode::OneDarkPro),
            "orng" => Some(ThemeMode::Orng),
            "osaka-jade" | "osaka" => Some(ThemeMode::OsakaJade),
            "palenight" => Some(ThemeMode::Palenight),
            "rosepine" | "rose-pine" => Some(ThemeMode::Rosepine),
            "shadesofpurple" | "shades-of-purple" => Some(ThemeMode::ShadesOfPurple),
            "solarized" => Some(ThemeMode::Solarized),
            "synthwave84" | "synthwave" => Some(ThemeMode::Synthwave84),
            "vercel" => Some(ThemeMode::Vercel),
            "vesper" => Some(ThemeMode::Vesper),
            "zenburn" => Some(ThemeMode::Zenburn),
            "system" | "auto" => Some(ThemeMode::System),
            _ => None,
        }
    }

    pub fn list_all() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            ("opencode", "OpenCode Dark (Modern)", "Domyślny, elegancki styl OpenCode"),
            ("graphite", "Graphite Grey (Matowy Szary)", "Mocno szary, stonowany i czytelny styl Zinc"),
            ("oled", "OLED Pure Black (Ochrona OLED)", "100% głęboka czerń, zero wypaleń ekranu"),
            ("catppuccin", "Catppuccin Mocha", "Subtelne pastelowe odcienie"),
            ("catppuccin-frappe", "Catppuccin Frappe", "Ciemniejszy wariant Catppuccin"),
            ("catppuccin-macchiato", "Catppuccin Macchiato", "Pośredni Catppuccin"),
            ("tokyo", "Tokyo Night", "Klimat nocnego Tokio (niebiesko-fioletowy)"),
            ("dracula", "Dracula", "Klasyczny ciemny motyw z fioletem"),
            ("nord", "Nord Frost", "Skandynawski arktyczny chłód"),
            ("cyberpunk", "Cyberpunk Neon", "Neonowy cyan i magenta"),
            ("gruvbox", "Gruvbox Dark", "Ciepłe retro kolory Ziemi"),
            ("monokai", "Monokai Pro", "Żywe, nasycone kolory kodu"),
            ("classic", "Classic Blue", "Tradycyjny niebieski styl"),
            ("oc-2", "OC-2", "Nowy OpenCode OC-2"),
            ("amoled", "AMOLED", "Czysta czerń AMOLED"),
            ("aura", "Aura", "Fioletowa aura"),
            ("ayu", "Ayu Dark", "Mirage Ayu"),
            ("carbonfox", "Carbonfox", "Ciemny Carbonfox"),
            ("cobalt2", "Cobalt2", "Niebieski Cobalt2"),
            ("cursor", "Cursor", "Motyw Cursor IDE"),
            ("everforest", "Everforest", "Zielony leśny"),
            ("flexoki", "Flexoki", "Ciepły Flexoki"),
            ("github", "GitHub Dark", "GitHub Dark"),
            ("kanagawa", "Kanagawa", "Fala Kanagawa"),
            ("lucent-orng", "Lucent Orng", "Pomarańczowy Lucent"),
            ("material", "Material", "Material Dark"),
            ("matrix", "Matrix", "Zielony Matrix"),
            ("mercury", "Mercury", "Szary Mercury"),
            ("nightowl", "Night Owl", "Nocna sowa"),
            ("one-dark", "One Dark", "Atom One Dark"),
            ("onedarkpro", "One Dark Pro", "One Dark Pro"),
            ("orng", "Orng", "Pomarańczowy Orng"),
            ("osaka-jade", "Osaka Jade", "Jadeit Osaka"),
            ("palenight", "Palenight", "Palenight"),
            ("rosepine", "Rosé Pine", "Rosé Pine"),
            ("shadesofpurple", "Shades of Purple", "Odcienie fioletu"),
            ("solarized", "Solarized Dark", "Solarized ciemny"),
            ("synthwave84", "Synthwave '84", "Neonowy Synthwave"),
            ("vercel", "Vercel", "Czarno-biały Vercel"),
            ("vesper", "Vesper", "Ciemny Vesper"),
            ("zenburn", "Zenburn", "Stonowany Zenburn"),
            ("system", "System (Terminal)", "Adaptuje się do tła terminala"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_theme_list_count_42() {
        let list = AppTheme::list_all();
        assert_eq!(list.len(), 42, "should have 42 themes");
        assert!(list.iter().any(|(id,_,_)| *id=="opencode"));
        assert!(list.iter().any(|(id,_,_)| *id=="system"));
    }
    #[test]
    fn test_theme_parse_and_get() {
        assert_eq!(AppTheme::parse("opencode"), Some(ThemeMode::OpenCodeDark));
        assert_eq!(AppTheme::parse("oled"), Some(ThemeMode::OledPureBlack));
        assert_eq!(AppTheme::parse("dracula"), Some(ThemeMode::Dracula));
        assert_eq!(AppTheme::parse("nieistnieje"), None);
        let t = AppTheme::get(&ThemeMode::Dracula);
        assert_eq!(t.name, "Dracula");
        assert_eq!(t.mode, ThemeMode::Dracula);
    }
    #[test]
    fn test_theme_get_all_modes() {
        for mode in [ThemeMode::OpenCodeDark, ThemeMode::Nord, ThemeMode::Cyberpunk, ThemeMode::Zenburn] {
            let th = AppTheme::get(&mode);
            assert!(!th.name.is_empty());
        }
    }
    #[test]
    fn test_theme_parse_aliases() {
        assert_eq!(AppTheme::parse("graphite"), Some(ThemeMode::GraphiteGrey));
        assert_eq!(AppTheme::parse("tokyo"), Some(ThemeMode::TokyoNight));
        assert_eq!(AppTheme::parse("catppuccin"), Some(ThemeMode::CatppuccinMocha));
    }
}
