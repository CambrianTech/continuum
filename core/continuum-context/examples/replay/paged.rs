//! Paged reads (`--paged-reads`): a read with no range answers with its FIRST PAGE and a line
//! saying how much more there is (`[lines 1-N of M; page with start_line/end_line]`), instead
//! of the whole file (plan line S1). Not a clip: she gets exactly what the tool says it gives,
//! and knows the rest is there.
//!
//! The page rule, with no constant of its own: an act's unranged reads share the view's
//! measured per-act headroom (`View::appended_per_act`, the median characters one act of this
//! epoch appended; the same quantity `--outline-results` decides by), each getting an equal
//! part; a page is the longest run of whole lines from line 1 that fits its part. A read that
//! fits is sent whole, and before the first epoch has measured a headroom every read is whole.
//!
//! The need for more pages, a LOWER bound: a later act of hers quoting a line past the page
//! (a line of at least [`super::outline::QUOTE_CHARS`] characters, as the outline proxy counts quotes; her
//! edits' `old_string` count, being tool input), or a ranged read of hers reaching past it. A
//! quoted line past the page charges one extra request appending the page that holds it, once
//! per page; a ranged read she made is a request she made anyway, so it costs nothing extra but
//! does mean the first page alone did not hold every line she used. The extra page is charged on
//! the request after the act that quoted it (the same tokens, one act later). A read of the
//! same file closes the earlier read's account: from then on that read supplies the lines.
//!
//! Conservative in one place: a turnover renders her results from the action log, restored
//! whole (`validate::Restored`), so a paged read is charged whole again at a start.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::outline::quotable;
use super::validate::{gutter, READ_TOOLS};

/// The read calls a request records.
#[derive(Default, Clone)]
pub struct Reads {
    /// The `tool_use` ids of every read with no range in the request: what paging would answer.
    pub whole: HashSet<String>,
    /// The paths the act it carries (its last assistant turn) read, ranged or not.
    pub paths: Vec<String>,
    /// The act's ranged reads: path, first line, and last line (`None`: to the end).
    pub ranged: Vec<(String, u64, Option<u64>)>,
}

fn path_of(input: &Value) -> Option<String> {
    input["file_path"].as_str().or(input["path"].as_str()).map(str::to_owned)
}

/// The read calls of `request`.
pub fn reads(request: &Value) -> Reads {
    let messages: Vec<&Value> = request["messages"].as_array().into_iter().flatten().collect();
    let calls = |message: &Value| -> Vec<Value> {
        message["content"].as_array().into_iter().flatten().filter(|part| part["type"] == "tool_use" && part["name"].as_str().is_some_and(|name| READ_TOOLS.contains(&name))).cloned().collect()
    };
    let ranged = |call: &Value| ["start_line", "end_line", "offset", "limit"].iter().any(|key| !call["input"][key].is_null());
    let mut reads = Reads::default();
    for message in messages.iter().filter(|m| m["role"] == "assistant") {
        for call in calls(message) {
            if let (false, Some(id)) = (ranged(&call), call["id"].as_str()) {
                reads.whole.insert(id.to_owned());
            }
        }
    }
    if let Some(last) = messages.iter().rfind(|m| m["role"] == "assistant") {
        for call in calls(last) {
            let Some(path) = path_of(&call["input"]) else { continue };
            if ranged(&call) {
                let start = call["input"]["start_line"].as_u64().unwrap_or(1); // unwrap_or: a range given only by its end starts at line 1
                reads.ranged.push((path.clone(), start, call["input"]["end_line"].as_u64()));
            }
            reads.paths.push(path);
        }
    }
    reads
}

/// One read answered with its first page.
#[derive(Clone)]
pub struct PagedRead {
    pub id: String,
    path: String,
    /// Lines in a page, and in the file.
    page: u64,
    total: u64,
    /// The file's lines as she was sent them, gutter included.
    lines: Vec<String>,
}

/// One tail piece with its unranged reads at their first page.
pub struct Paged {
    pub piece: String,
    pub reads: Vec<PagedRead>,
}

/// `piece` (one tail message, as sent) with each unranged read past its share of `headroom` at
/// its first page; `None` when it carries no such read or they all fit.
pub fn page(piece: &str, whole: &HashSet<String>, headroom: usize) -> Option<Paged> {
    let mut parts: Value = serde_json::from_str(piece).ok()?;
    let array = parts.as_array_mut()?;
    let indices: Vec<usize> = (0..array.len())
        .filter(|&index| array[index]["type"] == "tool_result" && array[index]["tool_use_id"].as_str().is_some_and(|id| whole.contains(id)) && array[index]["content"].is_string())
        .collect();
    let share = headroom / indices.len().max(1);
    let mut reads = Vec::new();
    for index in indices {
        let Some(content) = array[index]["content"].as_str().map(str::to_owned) else { continue };
        let Some((first, body)) = content.split_once('\n') else { continue };
        let Ok(mut header) = serde_json::from_str::<Value>(first) else { continue };
        if content.chars().count() <= share {
            continue;
        }
        let lines: Vec<String> = body.lines().map(str::to_owned).collect();
        let total = lines.len() as u64;
        let pager = |n: u64| format!("[lines 1-{n} of {total}; page with start_line/end_line]");
        let mut room = share.saturating_sub(first.chars().count() + pager(total).chars().count() + 2);
        let mut page = 0u64;
        for line in &lines {
            let cost = line.chars().count() + 1;
            if page > 0 && cost > room {
                break;
            }
            room = room.saturating_sub(cost);
            page += 1;
        }
        if page >= total {
            continue;
        }
        header["content"] = Value::String(format!("<{page} lines below>"));
        header["end_line"] = page.into();
        header["lines_returned"] = page.into();
        let shown = format!("{header}\n{}\n{}", lines[..page as usize].join("\n"), pager(page));
        let path = header["file_path"].as_str().unwrap_or_default().to_owned(); // unwrap_or_default: a read header always names its file
        let id = array[index]["tool_use_id"].as_str().unwrap_or_default().to_owned(); // unwrap_or_default: filtered on a string id above
        array[index]["content"] = Value::String(shown);
        reads.push(PagedRead { id, path, page, total, lines });
    }
    (!reads.is_empty()).then(|| Paged { piece: parts.to_string(), reads })
}

/// One paged read's account: what she used of it, until a read of the same file supersedes it.
struct Account {
    read: PagedRead,
    /// The first request whose act saw the page.
    from: usize,
    /// Each distinctive line's numbers in the file.
    quotes: HashMap<String, Vec<u64>>,
    fetched: HashSet<u64>,
    past: bool,
    /// A later act quoted a line of it or read a range of the file: evidence she used it.
    used: bool,
    open: bool,
}

/// Paged reads standing in her view, and what her later acts needed of them.
#[derive(Default)]
pub struct Pager {
    accounts: Vec<Account>,
    registered: HashSet<String>,
    pub extra: usize,
    last: Vec<String>,
}

impl Pager {
    /// `read` was answered with its first page on request `from - 1`.
    pub fn register(&mut self, from: usize, read: &PagedRead) {
        if !self.registered.insert(read.id.clone()) {
            return;
        }
        let mut quotes: HashMap<String, Vec<u64>> = HashMap::new();
        for (index, line) in read.lines.iter().enumerate() {
            let number = gutter(line).map_or(index as u64 + 1, |(number, _)| u64::from(number)); // map_or: a line without the gutter is numbered by position
            if let Some(code) = quotable(line) {
                quotes.entry(code).or_default().push(number);
            }
        }
        self.accounts.push(Account { read: read.clone(), from, quotes, fetched: HashSet::new(), past: false, used: false, open: true });
    }

    /// The act request `at` carries said `lines` and made `reads`: the pages it needed beyond
    /// the first, each returned once as the extra request's appended result.
    pub fn said(&mut self, at: usize, lines: &[String], reads: &Reads) -> Vec<String> {
        if lines == self.last.as_slice() {
            return Vec::new();
        }
        self.last = lines.to_vec();
        let mut pages = Vec::new();
        for account in self.accounts.iter_mut().filter(|account| account.open && at >= account.from) {
            let page = account.read.page;
            for line in lines {
                // A line that repeats in the file counts at its first place: a lower bound.
                let Some(&number) = account.quotes.get(line).and_then(|numbers| numbers.iter().min()) else { continue };
                account.used = true;
                if number <= page {
                    continue;
                }
                account.past = true;
                let index = (number - 1) / page;
                if account.fetched.insert(index) {
                    let (first, last) = (index * page + 1, ((index + 1) * page).min(account.read.total));
                    let body = account.read.lines[(first - 1) as usize..last as usize].join("\n");
                    pages.push(format!("[{} lines {first}-{last} of {}]\n{body}", account.read.path, account.read.total));
                }
            }
            for (path, _, end) in &reads.ranged {
                if *path == account.read.path {
                    account.used = true;
                    account.past |= end.is_none_or(|end| end > page); // none: a range with no end reads to the end of the file
                }
            }
            if reads.paths.contains(&account.read.path) {
                account.open = false;
            }
        }
        self.extra += pages.len();
        pages
    }

    /// Reads answered with a first page; of those, how many held every line she later used, and
    /// how many a later act used at all (a quote or a ranged read: the rest show no use either way).
    pub fn totals(&self) -> (usize, usize, usize) {
        let count = |keep: fn(&Account) -> bool| self.accounts.iter().filter(|account| keep(account)).count();
        (self.accounts.len(), count(|account| !account.past), count(|account| account.used))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(lines: usize) -> String {
        let body: String = (1..=lines).map(|n| format!("{n:>6} |         let value_number_{n} = compute_the_thing_for(index, {n});\n")).collect();
        format!("{{\"content\":\"<{lines} lines below>\",\"end_line\":{lines},\"file_path\":\"a.rs\",\"lines_returned\":{lines},\"start_line\":1,\"total_lines\":{lines}}}\n{body}")
    }

    fn act(id: &str, content: &str) -> String {
        serde_json::json!([{"type": "tool_result", "tool_use_id": id, "content": content}]).to_string()
    }

    // what this catches: a page that clips mid-line or drops the line telling her there is more,
    // a ranged read paged, or a read that fits paged at all.
    #[test]
    fn an_unranged_read_past_its_share_answers_with_its_first_page_and_the_rest_named() {
        let whole: HashSet<String> = ["w".to_owned()].into();
        let piece = act("w", &read(200));
        assert!(page(&piece, &whole, piece.len() * 2).is_none(), "a read that fits is whole");
        assert!(page(&act("r", &read(200)), &whole, 1_000).is_none(), "a ranged read is never paged");
        let Some(paged) = page(&piece, &whole, 2_000) else { panic!("an oversized unranged read pages") };
        let read = &paged.reads[0];
        assert!(read.page > 0 && read.page < 200 && read.total == 200);
        assert!(paged.piece.contains(&format!("[lines 1-{} of 200; page with start_line/end_line]", read.page)));
        assert!(paged.piece.contains(&format!("\\\"lines_returned\\\":{}", read.page)));
        assert!(!paged.piece.contains(&format!("value_number_{} ", read.page + 1)));
    }

    // what this catches: a page charged before her act saw it, twice for one page, for a line the
    // first page held, or after a later read of the file superseded it; a ranged read past the
    // page not counted against the first page's sufficiency.
    #[test]
    fn a_quote_past_the_page_charges_one_extra_page_once() {
        let whole: HashSet<String> = ["w".to_owned()].into();
        let Some(paged) = page(&act("w", &read(200)), &whole, 2_000) else { panic!("paged") };
        let first = &paged.reads[0];
        let quote = |n: u64| vec![format!("let value_number_{n} = compute_the_thing_for(index, {n});")];
        let mut pager = Pager::default();
        pager.register(3, first);
        pager.register(3, first);
        assert!(pager.said(2, &quote(first.page + 1), &Reads::default()).is_empty(), "the act before saw no page");
        assert!(pager.said(3, &quote(1), &Reads::default()).is_empty(), "the first page held it");
        assert_eq!(pager.said(4, &quote(first.page + 1), &Reads::default()).len(), 1);
        assert!(pager.said(5, &quote(first.page + 2), &Reads::default()).is_empty(), "the same page once");
        let reread = Reads { paths: vec!["a.rs".to_owned()], ..Reads::default() };
        pager.said(6, &quote(1), &reread);
        assert!(pager.said(7, &quote(2 * first.page + 1), &Reads::default()).is_empty(), "a later read supersedes");
        assert_eq!((pager.extra, pager.totals()), (1, (1, 0, 1)));
        let Some(again) = page(&act("v", &read(200)), &["v".to_owned()].into(), 2_000) else { panic!("paged") };
        pager.register(8, &again.reads[0]);
        let ranged = Reads { paths: vec!["a.rs".to_owned()], ranged: vec![("a.rs".to_owned(), again.reads[0].page + 1, None)], ..Reads::default() };
        assert!(pager.said(8, &quote(3), &ranged).is_empty());
        assert_eq!((pager.extra, pager.totals()), (1, (2, 0, 2)), "a ranged read past the page: not held, not charged");
    }
}
