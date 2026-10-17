//! HTML escaping for agent and tool text.
//!
//! Card resolves (`big`, `sub`, `lines`) and Suzy summaries are HTML: the field UI inserts them
//! with `innerHTML`. The inbox, team and research agents summarize email, Slack and news, so text
//! an outsider wrote can reach those strings. It is escaped here, on the server; only the
//! server's own markup (the `<b>•</b>` bullet, a bold title) stays markup.

/// Escape agent or tool text for HTML element content. Tags become visible text. Character
/// references the text already carries (`&amp;`, `&#39;`, as in Gmail snippets) are kept, so they
/// still render as the character and escaping twice changes nothing. Quotes pass through: this is
/// for element content only, never for attribute values.
pub fn escape(text: &str) -> String {
    escape_keeping(text, &[])
}

/// Agent-written HTML (Suzy's summary, the Mother's synthesis) reduced to the one tag those
/// prompts ask for: bare `<b>` and `</b>` stay markup, every other tag is escaped like [`escape`].
pub fn sanitize_inline(html: &str) -> String {
    escape_keeping(html, &["<b>", "</b>"])
}

fn escape_keeping(text: &str, allowed: &[&str]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        match ch {
            '<' => match allowed.iter().find(|t| rest.get(..t.len()).is_some_and(|p| p.eq_ignore_ascii_case(t))) {
                Some(tag) => {
                    out.push_str(tag);
                    rest = &rest[tag.len()..];
                    continue;
                }
                None => out.push_str("&lt;"),
            },
            '>' => out.push_str("&gt;"),
            '&' if starts_char_ref(rest) => out.push('&'),
            '&' => out.push_str("&amp;"),
            c => out.push(c),
        }
        rest = &rest[ch.len_utf8()..];
    }
    out
}

/// Whether `s` (starting at `&`) begins a complete character reference: `&name;`, `&#123;` or
/// `&#x1F;`. A reference only ever renders as text, so keeping it cannot open a tag.
fn starts_char_ref(s: &str) -> bool {
    let body = &s[1..];
    let Some(end) = body.char_indices().take(33).find(|&(_, c)| c == ';').map(|(i, _)| i) else {
        return false;
    };
    let name = &body[..end];
    match name.strip_prefix('#') {
        Some(num) => match num.strip_prefix(['x', 'X']) {
            Some(hex) => !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()),
            None => !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()),
        },
        None => {
            name.starts_with(|c: char| c.is_ascii_alphabetic()) && name.chars().all(|c| c.is_ascii_alphanumeric())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_in_agent_text_become_visible_text() {
        assert_eq!(escape("<script>alert(1)</script>"), "&lt;script&gt;alert(1)&lt;/script&gt;");
        assert_eq!(escape("Re: <img src=x onerror=alert(1)>"), "Re: &lt;img src=x onerror=alert(1)&gt;");
        assert_eq!(escape("<b>Fri</b> arrive"), "&lt;b&gt;Fri&lt;/b&gt; arrive");
    }

    #[test]
    fn character_references_are_kept_and_bare_ampersands_escaped() {
        assert_eq!(escape("Q&A with AT&T"), "Q&amp;A with AT&amp;T");
        assert_eq!(escape("Don&#39;t miss &amp; &#x1F600; &mdash;"), "Don&#39;t miss &amp; &#x1F600; &mdash;");
        assert_eq!(escape("&lt;script&gt;"), "&lt;script&gt;");
        assert_eq!(escape("& ;"), "&amp; ;");
        assert_eq!(escape("&#;&#x;&1a;&"), "&amp;#;&amp;#x;&amp;1a;&amp;");
        assert_eq!(escape("&lt no semicolon"), "&amp;lt no semicolon");
        let once = escape("a < b & <i>c</i> &amp; d");
        assert_eq!(escape(&once), once, "escaping twice changes nothing");
    }

    #[test]
    fn plain_text_and_unicode_pass_through() {
        assert_eq!(escape("Here's “the” plan — 3 meetings · 9am"), "Here's “the” plan — 3 meetings · 9am");
        assert_eq!(escape("café & ☕"), "café &amp; ☕");
        assert_eq!(escape(""), "");
    }

    #[test]
    fn sanitize_inline_keeps_only_bare_bold() {
        assert_eq!(sanitize_inline("You have <b>3 meetings</b> today."), "You have <b>3 meetings</b> today.");
        assert_eq!(sanitize_inline("<B>Loud</B>"), "<b>Loud</b>");
        assert_eq!(
            sanitize_inline("<b>Inbox</b> <img src=x onerror=alert(1)>"),
            "<b>Inbox</b> &lt;img src=x onerror=alert(1)&gt;"
        );
        assert_eq!(
            sanitize_inline("<b onmouseover=alert(1)>x</b><script>alert(1)</script>"),
            "&lt;b onmouseover=alert(1)&gt;x</b>&lt;script&gt;alert(1)&lt;/script&gt;"
        );
        assert_eq!(sanitize_inline("<br><a href=\"javascript:x\">y</a>"), "&lt;br&gt;&lt;a href=\"javascript:x\"&gt;y&lt;/a&gt;");
        assert_eq!(sanitize_inline("AT&T &amp; <b"), "AT&amp;T &amp; &lt;b");
    }
}
