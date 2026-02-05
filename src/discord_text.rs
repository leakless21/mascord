use poise::serenity_prelude as serenity;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

pub fn strip_bot_mentions(input: &str, bot_id: u64) -> String {
    let mention = format!("<@{}>", bot_id);
    let mention_nick = format!("<@!{}>", bot_id);

    input
        .replace(&mention, "")
        .replace(&mention_nick, "")
        .trim()
        .to_string()
}

pub fn extract_message_text(message: &serenity::Message) -> String {
    let mut parts = Vec::new();

    let content = message.content.trim();
    if !content.is_empty() {
        parts.push(content.to_string());
    }

    for embed in &message.embeds {
        if let Some(title) = &embed.title {
            let title = title.trim();
            if !title.is_empty() {
                parts.push(title.to_string());
            }
        }

        if let Some(description) = &embed.description {
            let description = description.trim();
            if !description.is_empty() {
                parts.push(description.to_string());
            }
        }

        for field in &embed.fields {
            let name = field.name.trim();
            let value = field.value.trim();

            if name.is_empty() && value.is_empty() {
                continue;
            }

            if name.is_empty() {
                parts.push(value.to_string());
                continue;
            }

            if value.is_empty() {
                parts.push(name.to_string());
                continue;
            }

            parts.push(format!("{}: {}", name, value));
        }
    }

    parts.join("\n")
}

/// Convert Markdown into Discord-friendly text.
/// Unsupported Markdown elements (tables/images/html/etc.) are degraded into readable text.
pub fn format_for_discord(input: &str) -> String {
    let normalized = preprocess_tables(input);
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(&normalized, options);
    let mut renderer = DiscordRenderer::new();

    for event in parser {
        renderer.handle_event(event);
    }

    renderer.finish()
}

fn preprocess_tables(input: &str) -> String {
    let mut output: Vec<String> = Vec::new();
    let mut lines = input.lines().peekable();
    let mut in_code_block = false;

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            output.push(line.to_string());
            continue;
        }

        if in_code_block {
            output.push(line.to_string());
            continue;
        }

        if line.contains('|') {
            if let Some(next_line) = lines.peek() {
                if looks_like_table_separator(next_line) {
                    let headers = split_table_row(line);
                    lines.next();
                    let mut rows = Vec::new();
                    while let Some(peek) = lines.peek() {
                        if !peek.contains('|') {
                            break;
                        }
                        let row_line = lines.next().unwrap();
                        if row_line.trim().is_empty() {
                            break;
                        }
                        rows.push(split_table_row(&row_line));
                    }
                    output.extend(format_table_block(headers, rows));
                    continue;
                }
            }
        }

        output.push(line.to_string());
    }

    output.join("\n")
}

#[derive(Debug, Clone)]
enum ListKind {
    Unordered,
    Ordered { next_index: u64 },
}

#[derive(Debug, Clone)]
struct ListState {
    kind: ListKind,
}

#[derive(Debug, Clone)]
struct LinkState {
    destination: String,
    is_image: bool,
}

#[derive(Debug, Default)]
struct TableState {
    head_rows: Vec<Vec<String>>,
    body_rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
    in_head: bool,
    in_row: bool,
    in_cell: bool,
}

struct DiscordRenderer {
    output_stack: Vec<String>,
    at_line_start: bool,
    in_code_block: bool,
    blockquote_level: usize,
    list_stack: Vec<ListState>,
    in_list_item: bool,
    list_item_continuation_indent: String,
    link_stack: Vec<LinkState>,
    table_state: Option<TableState>,
}

impl DiscordRenderer {
    fn new() -> Self {
        Self {
            output_stack: vec![String::new()],
            at_line_start: true,
            in_code_block: false,
            blockquote_level: 0,
            list_stack: Vec::new(),
            in_list_item: false,
            list_item_continuation_indent: String::new(),
            link_stack: Vec::new(),
            table_state: None,
        }
    }

    fn finish(mut self) -> String {
        let mut output = self.output_stack.pop().unwrap_or_default();
        output = output.trim_end_matches(['\n', ' ']).to_string();
        output
    }

    fn handle_event(&mut self, event: Event) {
        if self.handle_table_event(&event) {
            return;
        }

        match event {
            Event::Start(tag) => self.handle_start_tag(tag),
            Event::End(tag) => self.handle_end_tag(tag),
            Event::Text(text) => self.write_text(&text),
            Event::Code(code) => self.write_inline_code(&code),
            Event::SoftBreak | Event::HardBreak => self.new_line(),
            Event::Rule => {
                self.new_line();
                self.write_raw("---");
                self.new_line();
            }
            Event::Html(_) | Event::InlineHtml(_) | Event::FootnoteReference(_) => {}
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                self.write_text(marker);
            }
        }
    }

    fn handle_start_tag(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { .. } => {
                self.ensure_line_start();
                self.write_raw("**");
            }
            Tag::BlockQuote => {
                self.blockquote_level += 1;
                self.ensure_line_start();
            }
            Tag::List(start) => {
                let kind = match start {
                    Some(value) => ListKind::Ordered { next_index: value as u64 },
                    None => ListKind::Unordered,
                };
                self.list_stack.push(ListState { kind });
            }
            Tag::Item => {
                self.start_list_item();
            }
            Tag::Emphasis => self.write_raw("*"),
            Tag::Strong => self.write_raw("**"),
            Tag::Strikethrough => self.write_raw("~~"),
            Tag::CodeBlock(kind) => self.start_code_block(kind),
            Tag::Link { dest_url, .. } => {
                self.link_stack.push(LinkState { destination: dest_url.to_string(), is_image: false });
                self.output_stack.push(String::new());
            }
            Tag::Image { dest_url, .. } => {
                self.link_stack.push(LinkState { destination: dest_url.to_string(), is_image: true });
                self.output_stack.push(String::new());
            }
            Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
            Tag::FootnoteDefinition(_) => {}
            Tag::HtmlBlock | Tag::MetadataBlock(_) => {}
        }
    }

    fn handle_end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.new_line();
            }
            TagEnd::Heading { .. } => {
                self.write_raw("**");
                self.new_line();
            }
            TagEnd::BlockQuote => {
                if self.blockquote_level > 0 {
                    self.blockquote_level -= 1;
                }
                self.new_line();
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
                self.new_line();
            }
            TagEnd::Item => {
                self.in_list_item = false;
                self.list_item_continuation_indent.clear();
                self.new_line();
            }
            TagEnd::Emphasis => self.write_raw("*"),
            TagEnd::Strong => self.write_raw("**"),
            TagEnd::Strikethrough => self.write_raw("~~"),
            TagEnd::CodeBlock => self.end_code_block(),
            TagEnd::Link | TagEnd::Image => self.end_link(),
            TagEnd::Table | TagEnd::TableHead | TagEnd::TableRow | TagEnd::TableCell => {}
            TagEnd::FootnoteDefinition => {}
            TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
        }
    }

    fn handle_table_event(&mut self, event: &Event) -> bool {
        let Some(state) = &mut self.table_state else {
            if matches!(event, Event::Start(Tag::Table(_))) {
                self.table_state = Some(TableState::default());
                return true;
            }
            return false;
        };

        match event {
            Event::Start(Tag::Table(_)) => true,
            Event::End(TagEnd::Table) => {
                let table_state = self.table_state.take().unwrap_or_default();
                let headers = if !table_state.head_rows.is_empty() {
                    table_state.head_rows[0].clone()
                } else if !table_state.body_rows.is_empty() {
                    table_state.body_rows[0].clone()
                } else {
                    Vec::new()
                };
                let rows = if !table_state.head_rows.is_empty() {
                    table_state.body_rows
                } else if table_state.body_rows.len() > 1 {
                    table_state.body_rows[1..].to_vec()
                } else {
                    Vec::new()
                };
                let lines = format_table_block(headers, rows);
                if !lines.is_empty() {
                    self.ensure_line_start();
                    self.write_raw(&lines.join("\n"));
                    self.new_line();
                }
                true
            }
            Event::Start(Tag::TableHead) => {
                state.in_head = true;
                true
            }
            Event::End(TagEnd::TableHead) => {
                state.in_head = false;
                true
            }
            Event::Start(Tag::TableRow) => {
                state.current_row = Vec::new();
                state.in_row = true;
                true
            }
            Event::End(TagEnd::TableRow) => {
                if state.in_row && !state.current_row.is_empty() {
                    if state.in_head {
                        state.head_rows.push(state.current_row.clone());
                    } else {
                        state.body_rows.push(state.current_row.clone());
                    }
                }
                state.in_row = false;
                true
            }
            Event::Start(Tag::TableCell) => {
                state.current_cell.clear();
                state.in_cell = true;
                true
            }
            Event::End(TagEnd::TableCell) => {
                let cell = state.current_cell.trim().to_string();
                state.current_row.push(cell);
                state.current_cell.clear();
                state.in_cell = false;
                true
            }
            Event::Text(text) => {
                if state.in_cell {
                    state.current_cell.push_str(text);
                }
                true
            }
            Event::Code(code) => {
                if state.in_cell {
                    state.current_cell.push_str(code);
                }
                true
            }
            Event::SoftBreak | Event::HardBreak => {
                if state.in_cell {
                    state.current_cell.push(' ');
                }
                true
            }
            _ => true,
        }
    }

    fn write_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.in_code_block {
            self.write_raw(text);
            return;
        }
        self.ensure_line_start();
        self.write_raw(text);
    }

    fn write_inline_code(&mut self, code: &str) {
        if self.in_code_block {
            self.write_raw(code);
            return;
        }
        let escaped = code.replace('`', "\\`");
        self.ensure_line_start();
        self.write_raw("`");
        self.write_raw(&escaped);
        self.write_raw("`");
    }

    fn start_code_block(&mut self, kind: CodeBlockKind) {
        self.ensure_line_start();
        match kind {
            CodeBlockKind::Fenced(info) => {
                let info = info.trim();
                if info.is_empty() {
                    self.write_raw("```");
                } else {
                    self.write_raw("```");
                    self.write_raw(info);
                }
            }
            CodeBlockKind::Indented => {
                self.write_raw("```");
            }
        }
        self.new_line();
        self.in_code_block = true;
    }

    fn end_code_block(&mut self) {
        self.in_code_block = false;
        if !self.at_line_start {
            self.new_line();
        }
        self.write_raw("```");
        self.new_line();
    }

    fn end_link(&mut self) {
        let link_text = self.output_stack.pop().unwrap_or_default();
        let link_state = self.link_stack.pop();
        if let Some(state) = link_state {
            let text = link_text.trim();
            if state.is_image {
                if text.is_empty() {
                    self.write_text(&state.destination);
                } else {
                    self.write_text(&format!("{} ({})", text, state.destination));
                }
            } else if text.is_empty() || text == state.destination {
                self.write_text(&state.destination);
            } else {
                self.write_text(&format!("{} ({})", text, state.destination));
            }
        } else {
            self.write_text(&link_text);
        }
    }

    fn start_list_item(&mut self) {
        if !self.at_line_start {
            self.new_line();
        }
        self.ensure_blockquote_prefix();
        let depth = self.list_stack.len();
        let indent = "  ".repeat(depth.saturating_sub(1));
        let prefix = match self.list_stack.last_mut().map(|state| &mut state.kind) {
            Some(ListKind::Ordered { next_index }) => {
                let current = *next_index;
                *next_index = next_index.saturating_add(1);
                format!("{}. ", current)
            }
            _ => "- ".to_string(),
        };

        self.write_raw(&indent);
        self.write_raw(&prefix);
        self.in_list_item = true;
        self.list_item_continuation_indent = format!("{}{}", indent, " ".repeat(prefix.len()));
        self.at_line_start = false;
    }

    fn ensure_line_start(&mut self) {
        if !self.at_line_start {
            return;
        }
        self.ensure_blockquote_prefix();
        if self.in_list_item {
            let indent = self.list_item_continuation_indent.clone();
            self.write_raw(&indent);
        }
        self.at_line_start = false;
    }

    fn ensure_blockquote_prefix(&mut self) {
        if self.blockquote_level == 0 || !self.at_line_start {
            return;
        }
        self.write_raw(&"> ".repeat(self.blockquote_level));
    }

    fn new_line(&mut self) {
        self.write_raw("\n");
        self.at_line_start = true;
    }

    fn write_raw(&mut self, value: &str) {
        if let Some(target) = self.output_stack.last_mut() {
            target.push_str(value);
        }
    }
}

fn looks_like_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    let mut has_dash = false;
    for ch in trimmed.chars() {
        match ch {
            '-' => has_dash = true,
            '|' | ':' | ' ' | '\t' => {}
            _ => return false,
        }
    }

    has_dash
}

fn split_table_row(line: &str) -> Vec<String> {
    let mut cells: Vec<String> = line.split('|').map(|cell| cell.trim().to_string()).collect();

    while matches!(cells.first(), Some(cell) if cell.is_empty()) {
        cells.remove(0);
    }
    while matches!(cells.last(), Some(cell) if cell.is_empty()) {
        cells.pop();
    }

    cells
}

fn format_table_block(headers: Vec<String>, rows: Vec<Vec<String>>) -> Vec<String> {
    let mut lines = Vec::new();
    let header_line = headers
        .iter()
        .map(|h| h.trim())
        .filter(|h| !h.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    let use_headers = !header_line.is_empty();

    if use_headers {
        lines.push(format!("**{}**", header_line));
    }

    for row in rows {
        if row.is_empty() {
            continue;
        }

        let mut parts = Vec::new();
        if use_headers {
            let max_cells = row.len().max(headers.len());
            for idx in 0..max_cells {
                let cell = row.get(idx).map(|c| c.trim()).unwrap_or("");
                if cell.is_empty() {
                    continue;
                }
                if let Some(header) = headers.get(idx).map(|h| h.trim()).filter(|h| !h.is_empty())
                {
                    parts.push(format!("{}: {}", header, cell));
                } else {
                    parts.push(cell.to_string());
                }
            }
        } else {
            for cell in row {
                let cell = cell.trim();
                if !cell.is_empty() {
                    parts.push(cell.to_string());
                }
            }
        }

        if !parts.is_empty() {
            lines.push(format!("- {}", parts.join(" | ")));
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::format_for_discord;

    #[test]
    fn converts_headings_and_basic_formatting() {
        let input = "# Title\n\nSome **bold** and *italic* text.";
        let output = format_for_discord(input);
        assert!(output.contains("**Title**"));
        assert!(output.contains("**bold**"));
        assert!(output.contains("*italic*"));
    }

    #[test]
    fn preserves_code_blocks_and_inline_code() {
        let input = "Inline `code` and:\n```rust\nfn main() {}\n```";
        let output = format_for_discord(input);
        assert!(output.contains("`code`"));
        assert!(output.contains("```rust"));
        assert!(output.contains("fn main() {}"));
    }

    #[test]
    fn converts_tables_to_bulleted_blocks() {
        let input = "| Name | Age |\n| --- | --- |\n| Ada | 36 |\n| Bob | 41 |";
        let output = format_for_discord(input);
        assert!(output.contains("**Name | Age**"), "output: {output}");
        assert!(output.contains("- Name: Ada | Age: 36"), "output: {output}");
        assert!(output.contains("- Name: Bob | Age: 41"), "output: {output}");
    }

    #[test]
    fn converts_links_and_images() {
        let input = "[Rust](https://www.rust-lang.org) ![Logo](https://example.com/logo.png)";
        let output = format_for_discord(input);
        assert!(output.contains("Rust (https://www.rust-lang.org)"));
        assert!(output.contains("Logo (https://example.com/logo.png)"));
    }

    #[test]
    fn preserves_lists_and_blockquotes() {
        let input = "> Quote\n\n- One\n- Two\n1. First\n2. Second";
        let output = format_for_discord(input);
        assert!(output.contains("> Quote"));
        assert!(output.contains("- One"));
        assert!(output.contains("1. First"));
    }

    #[test]
    fn handles_task_lists() {
        let input = "- [x] Done\n- [ ] Todo";
        let output = format_for_discord(input);
        assert!(output.contains("[x] Done"));
        assert!(output.contains("[ ] Todo"));
    }
}
