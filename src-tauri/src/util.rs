/// 剥离 Markdown 标记，返回纯文本用于系统通知显示
pub(crate) fn strip_markdown_for_notify(md: &str) -> String {
    let mut result = String::with_capacity(md.len());
    let mut chars = md.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '#' | '*' | '_' | '~' | '`' => {
                // 跳过连续相同字符
                while chars.peek() == Some(&c) { chars.next(); }
            }
            '!' => {
                // 图片 ![]() 跳过 !
                if chars.peek() == Some(&'[') { /* skip */ } else { result.push(c); }
            }
            '[' => {
                // 链接 []() 保留文字跳过括号
                let mut link_text = String::new();
                for lc in chars.by_ref() {
                    if lc == ']' { break; }
                    link_text.push(lc);
                }
                // 跳过 (url)
                if chars.peek() == Some(&'(') {
                    chars.next();
                    for lc in chars.by_ref() {
                        if lc == ')' { break; }
                    }
                }
                result.push_str(&link_text);
            }
            '>' => {
                // 跳过行首 > 引用标记
                if result.chars().last().map_or(true, |c| c == '\n' || c == ' ') {
                    while chars.peek() == Some(&' ') { chars.next(); }
                } else {
                    result.push(c);
                }
            }
            _ => result.push(c),
        }
    }
    // 合并空行、截断
    let lines: Vec<&str> = result.lines().filter(|l| !l.trim().is_empty()).collect();
    let text = lines.join(" ");
    let text = text.trim();
    // 按字符（非字节）截断：中文每字 3 字节，直接 &text[..200] 会落在多字节字符中间导致 panic，
    // 使提醒线程崩溃、提醒功能永久失效；改为按字符数截断，保证切在字符边界
    const MAX_CHARS: usize = 200;
    if text.chars().count() > MAX_CHARS {
        let truncated: String = text.chars().take(MAX_CHARS).collect();
        format!("{}...", truncated)
    } else {
        text.to_string()
    }
}
