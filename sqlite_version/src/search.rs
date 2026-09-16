use anyhow::{anyhow, Result};
use reqwest::Client;

pub struct WebSearch;

impl WebSearch {
    /// Wykonuje szybkie wyszukiwanie w internecie i zwraca zwięzłe podsumowanie wyników
    pub async fn search(query: &str) -> Result<String> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()?;

        let encoded = urlencoding_simple(query);
        let url = format!("https://html.duckduckgo.com/html/?q={encoded}");

        let resp = client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("Błąd wyszukiwarki sieciowej (status: {})", resp.status()));
        }

        let html = resp.text().await?;
        let results = parse_ddg_html(&html);

        if results.is_empty() {
            return Ok(format!("Brak wyników wyszukiwania w sieci dla zapytania: '{query}'"));
        }

        let mut output = format!("🌐 Wyniki wyszukiwania w sieci dla: '{query}':\n\n");
        for (i, (title, snippet, link)) in results.iter().take(5).enumerate() {
            output.push_str(&format!("{}. **{}**\n   {}\n   🔗 {}\n\n", i + 1, title, snippet, link));
        }

        Ok(output)
    }
}

fn parse_ddg_html(html: &str) -> Vec<(String, String, String)> {
    let mut results = Vec::new();

    // Prosty, szybki parser bloków wyników DuckDuckGo HTML bez ciężkich parserów DOM
    let mut cursor = 0;
    while let Some(result_pos) = html[cursor..].find("class=\"result__body") {
        let block_start = cursor + result_pos;
        let block_end = if let Some(end_pos) = html[block_start..].find("</div>\n</div>") {
            block_start + end_pos
        } else {
            block_start + 1500.min(html.len() - block_start)
        };

        let block = &html[block_start..block_end];
        cursor = block_end;

        // Wyciągnij tytuł i link z <a class="result__snippet" ...> / <a class="result__url" ...>
        let title = extract_tag_content(block, "result__snippet").or_else(|| extract_tag_content(block, "result__title"));
        let snippet = extract_tag_content(block, "result__snippet");
        let link = extract_href(block);

        if let (Some(t), Some(s), Some(l)) = (title, snippet, link) {
            let clean_t = strip_html_tags(&t);
            let clean_s = strip_html_tags(&s);
            if !clean_t.is_empty() && !clean_s.is_empty() {
                results.push((clean_t, clean_s, l));
            }
        }

        if results.len() >= 6 {
            break;
        }
    }

    results
}

fn extract_tag_content(block: &str, class_name: &str) -> Option<String> {
    if let Some(pos) = block.find(class_name) {
        if let Some(tag_start) = block[pos..].find('>') {
            let content_start = pos + tag_start + 1;
            if let Some(tag_end) = block[content_start..].find("</a>") {
                return Some(block[content_start..content_start + tag_end].to_string());
            } else if let Some(tag_end) = block[content_start..].find("</div>") {
                return Some(block[content_start..content_start + tag_end].to_string());
            }
        }
    }
    None
}

fn extract_href(block: &str) -> Option<String> {
    if let Some(href_pos) = block.find("href=\"") {
        let link_start = href_pos + 6;
        if let Some(quote_end) = block[link_start..].find('"') {
            let raw_link = &block[link_start..link_start + quote_end];
            if raw_link.starts_with("//duckduckgo.com/l/?uddg=") {
                if let Some(clean_url) = raw_link.split("uddg=").nth(1) {
                    if let Some(url_end) = clean_url.split('&').next() {
                        return Some(urldecode_simple(url_end));
                    }
                }
            }
            return Some(raw_link.to_string());
        }
    }
    None
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;

    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }

    out.replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

fn urlencoding_simple(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{:02X}", b)),
        }
    }
    encoded
}

fn urldecode_simple(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }

    result
}
