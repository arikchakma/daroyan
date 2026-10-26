use serde_json::{Map, Value, json};

const NODES: [&str; 11] = [
    "doc",
    "paragraph",
    "heading",
    "text",
    "hardBreak",
    "bulletList",
    "orderedList",
    "listItem",
    "blockquote",
    "codeBlock",
    "horizontalRule",
];
const MARKS: [&str; 5] = ["bold", "italic", "strike", "code", "link"];
const LINK_SCHEMES: [&str; 3] = ["https://", "http://", "mailto:"];
const HEADING_LEVELS: [u64; 2] = [2, 3];
const MAX_DEPTH: usize = 12;
const MAX_JSON_LENGTH: usize = 200_000;

#[derive(Debug, PartialEq, Eq)]
pub enum RichTextError {
    Invalid,
    Empty,
    TooLong,
}

pub fn paragraph(text: &str) -> Value {
    json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }] })
}

fn from_plain_text(text: &str) -> Value {
    let paragraphs: Vec<Value> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| {
            json!({ "type": "paragraph", "content": [{ "type": "text", "text": paragraph }] })
        })
        .collect();
    json!({ "type": "doc", "content": paragraphs })
}

pub fn read_document(value: &Value, max_chars: usize) -> Result<Value, RichTextError> {
    let converted;
    let value = match value.as_str() {
        Some(text) => {
            converted = from_plain_text(text);
            &converted
        }
        None => value,
    };
    if value.to_string().len() > MAX_JSON_LENGTH {
        return Err(RichTextError::TooLong);
    }
    if value.get("type").and_then(Value::as_str) != Some("doc") {
        return Err(RichTextError::Invalid);
    }
    let document = clean_node(value, 0)?;
    let text = to_plain_text(&document);
    if text.trim().is_empty() {
        return Err(RichTextError::Empty);
    }
    if text.chars().count() > max_chars {
        return Err(RichTextError::TooLong);
    }
    Ok(document)
}

fn clean_node(node: &Value, depth: usize) -> Result<Value, RichTextError> {
    if depth > MAX_DEPTH {
        return Err(RichTextError::Invalid);
    }
    let kind = node
        .get("type")
        .and_then(Value::as_str)
        .ok_or(RichTextError::Invalid)?;
    if !NODES.contains(&kind) {
        return Err(RichTextError::Invalid);
    }
    let mut clean = Map::new();
    clean.insert("type".to_string(), json!(kind));
    if kind == "text" {
        let text = node
            .get("text")
            .and_then(Value::as_str)
            .ok_or(RichTextError::Invalid)?;
        clean.insert("text".to_string(), json!(text));
    }
    if kind == "heading" {
        let level = node.pointer("/attrs/level").and_then(Value::as_u64).unwrap_or(2);
        if !HEADING_LEVELS.contains(&level) {
            return Err(RichTextError::Invalid);
        }
        clean.insert("attrs".to_string(), json!({ "level": level }));
    }
    if kind == "orderedList"
        && let Some(start) = node.pointer("/attrs/start").and_then(Value::as_u64)
    {
        clean.insert("attrs".to_string(), json!({ "start": start }));
    }
    if let Some(marks) = node.get("marks") {
        let marks = marks
            .as_array()
            .ok_or(RichTextError::Invalid)?
            .iter()
            .map(clean_mark)
            .collect::<Result<Vec<_>, _>>()?;
        if !marks.is_empty() {
            clean.insert("marks".to_string(), Value::Array(marks));
        }
    }
    if let Some(content) = node.get("content") {
        let content = content
            .as_array()
            .ok_or(RichTextError::Invalid)?
            .iter()
            .map(|child| clean_node(child, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        clean.insert("content".to_string(), Value::Array(content));
    }
    Ok(Value::Object(clean))
}

fn clean_mark(mark: &Value) -> Result<Value, RichTextError> {
    let kind = mark
        .get("type")
        .and_then(Value::as_str)
        .ok_or(RichTextError::Invalid)?;
    if !MARKS.contains(&kind) {
        return Err(RichTextError::Invalid);
    }
    if kind != "link" {
        return Ok(json!({ "type": kind }));
    }
    let href = mark
        .pointer("/attrs/href")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let is_safe = LINK_SCHEMES
        .iter()
        .any(|scheme| href.to_ascii_lowercase().starts_with(scheme));
    if !is_safe {
        return Err(RichTextError::Invalid);
    }
    Ok(json!({ "type": "link", "attrs": { "href": href } }))
}

pub fn to_plain_text(node: &Value) -> String {
    let mut lines: Vec<String> = Vec::new();
    collect_blocks(node, &mut lines);
    lines.join("\n").trim().to_string()
}

fn collect_blocks(node: &Value, lines: &mut Vec<String>) {
    match node.get("type").and_then(Value::as_str) {
        Some("paragraph" | "heading" | "codeBlock") => lines.push(inline_text(node)),
        _ => {
            for child in
                node.get("content").and_then(Value::as_array).into_iter().flatten()
            {
                collect_blocks(child, lines);
            }
        }
    }
}

fn inline_text(node: &Value) -> String {
    match node.get("type").and_then(Value::as_str) {
        Some("text") => node
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Some("hardBreak") => "\n".to_string(),
        _ => node
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(inline_text)
            .collect(),
    }
}

pub fn to_html(node: &Value) -> String {
    let content = || -> String {
        node.get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(to_html)
            .collect()
    };
    match node.get("type").and_then(Value::as_str) {
        Some("doc") => content(),
        Some("paragraph") => format!("<p>{}</p>", content()),
        Some("heading") => {
            let level = match node.pointer("/attrs/level").and_then(Value::as_u64) {
                Some(3) => 3,
                _ => 2,
            };
            format!("<h{level}>{}</h{level}>", content())
        }
        Some("bulletList") => format!("<ul>{}</ul>", content()),
        Some("orderedList") => format!("<ol>{}</ol>", content()),
        Some("listItem") => format!("<li>{}</li>", content()),
        Some("blockquote") => format!("<blockquote>{}</blockquote>", content()),
        Some("codeBlock") => format!("<pre><code>{}</code></pre>", content()),
        Some("horizontalRule") => "<hr>".to_string(),
        Some("hardBreak") => "<br>".to_string(),
        Some("text") => render_text(node),
        _ => String::new(),
    }
}

fn render_text(node: &Value) -> String {
    let text = escape_html(node.get("text").and_then(Value::as_str).unwrap_or_default());
    node.get("marks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .fold(text, |html, mark| match mark.get("type").and_then(Value::as_str) {
            Some("bold") => format!("<strong>{html}</strong>"),
            Some("italic") => format!("<em>{html}</em>"),
            Some("strike") => format!("<s>{html}</s>"),
            Some("code") => format!("<code>{html}</code>"),
            Some("link") => {
                let href = mark
                    .pointer("/attrs/href")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let is_safe = LINK_SCHEMES
                    .iter()
                    .any(|scheme| href.to_ascii_lowercase().starts_with(scheme));
                if is_safe {
                    format!(
                        "<a href=\"{}\" rel=\"noopener noreferrer nofollow\">{html}</a>",
                        escape_html(href)
                    )
                } else {
                    html
                }
            }
            _ => html,
        })
}

pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_known_formatting_and_reads_its_text() {
        let document = json!({ "type": "doc", "content": [
            { "type": "paragraph", "content": [
                { "type": "text", "text": "pg_wal", "marks": [{ "type": "code" }] },
                { "type": "text", "text": " filled up.", "extra": true }
            ] },
            { "type": "bulletList", "content": [{ "type": "listItem", "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "Freed space", "marks": [{ "type": "link", "attrs": { "href": "https://daroyan.dev", "target": "_blank" } }] }] }
            ] }] }
        ] });
        let clean = read_document(&document, 100).unwrap();
        assert_eq!(to_plain_text(&clean), "pg_wal filled up.\nFreed space");
        assert!(clean.pointer("/content/0/content/1/extra").is_none());
        assert_eq!(
            clean.pointer("/content/1/content/0/content/0/content/0/marks/0/attrs"),
            Some(&json!({ "href": "https://daroyan.dev" }))
        );
    }

    #[test]
    fn reads_plain_text_as_paragraphs() {
        let clean = read_document(&json!("Logins fail.\n\n Rolled back. "), 100).unwrap();
        assert_eq!(to_plain_text(&clean), "Logins fail.\nRolled back.");
    }

    #[test]
    fn rejects_unsafe_or_empty_documents() {
        let link = |href: &str| {
            json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [
                { "type": "text", "text": "x", "marks": [{ "type": "link", "attrs": { "href": href } }] }
            ] }] })
        };
        assert_eq!(
            read_document(&link("javascript:alert(1)"), 100),
            Err(RichTextError::Invalid)
        );
        assert!(read_document(&link("HTTPS://daroyan.dev"), 100).is_ok());
        let image = json!({ "type": "doc", "content": [{ "type": "image", "attrs": { "src": "x" } }] });
        assert_eq!(read_document(&image, 100), Err(RichTextError::Invalid));
        assert_eq!(read_document(&json!("  \n\n "), 100), Err(RichTextError::Empty));
        let heading = |level: u64| json!({ "type": "doc", "content": [{ "type": "heading", "attrs": { "level": level }, "content": [{ "type": "text", "text": "Impact" }] }] });
        assert!(read_document(&heading(2), 100).is_ok());
        assert_eq!(read_document(&heading(1), 100), Err(RichTextError::Invalid));
        let blank = json!({ "type": "doc", "content": [{ "type": "paragraph" }] });
        assert_eq!(read_document(&blank, 100), Err(RichTextError::Empty));
        assert_eq!(read_document(&paragraph("abcdef"), 5), Err(RichTextError::TooLong));
    }

    #[test]
    fn renders_html_and_escapes_text() {
        let document = json!({ "type": "doc", "content": [
            { "type": "heading", "attrs": { "level": 3 }, "content": [{ "type": "text", "text": "Impact" }] },
            { "type": "paragraph", "content": [
                { "type": "text", "text": "<script>", "marks": [{ "type": "bold" }, { "type": "code" }] },
                { "type": "text", "text": " docs", "marks": [{ "type": "link", "attrs": { "href": "https://x.dev/?a=1&b=\"2\"" } }] },
                { "type": "text", "text": " bad", "marks": [{ "type": "link", "attrs": { "href": "javascript:alert(1)" } }] }
            ] },
            { "type": "bulletList", "content": [{ "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "a & b" }] }] }] }
        ] });
        assert_eq!(
            to_html(&document),
            "<h3>Impact</h3><p><code><strong>&lt;script&gt;</strong></code><a href=\"https://x.dev/?a=1&amp;b=&quot;2&quot;\" rel=\"noopener noreferrer nofollow\"> docs</a> bad</p><ul><li><p>a &amp; b</p></li></ul>"
        );
    }
}
