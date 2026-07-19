//! Static-HTML eye — parse a rendered HTML artifact into a perception [`ProbeNode`]
//! tree so the headless eval core can grade a persona's UI with no browser connected.
//!
//! `perception/observe` is a *Provided* capability: the substrate routes it to a
//! browser-capable eye-node adapter, and fails loud when none is connected. The eval
//! core is headless — so a live render can't be observed there. But a from-scratch
//! web-dev task's `ui_checks` (does the DOM carry an `<h1>` reading "Sign in", ≥2
//! `<input>`s, a Submit button) are a STATIC-DOM question: a pure HTML parse answers
//! them the way a browser's accessibility tree would. This module is that static eye —
//! `file → ObserveResult{structure}` — so [`grade_ui`](super::scoring::grade_ui) scores
//! a real render deterministically, no browser dependency, reproducibly.
//!
//! The full browser eye ([`perception/observe`](super::ObserveCommand)) remains the path
//! for JS-rendered / dynamic pages — a persona's live seeing-and-iterating loop. This is
//! the grader's eye for a static artifact, not a replacement for either.

use std::collections::HashMap;
use std::path::Path;

use super::{ObserveResult, ProbeNode};

/// Read an HTML file and observe it into an [`ObserveResult`]. On read failure returns
/// `success: false` with the reason (grading treats that as a miss, never a false pass).
pub fn observe_file(path: &Path) -> ObserveResult {
    match std::fs::read_to_string(path) {
        Ok(html) => observe_html(&html, Some(format!("file://{}", path.display()))),
        Err(e) => ObserveResult {
            success: false,
            url: Some(format!("file://{}", path.display())),
            title: None,
            image: None,
            structure: None,
            error: Some(format!(
                "static-html eye: cannot read '{}': {e}",
                path.display()
            )),
        },
    }
}

/// Parse an HTML string into an [`ObserveResult`] with a [`ProbeNode`] structure tree.
/// html5ever-backed (via `scraper`), so malformed model output parses the way a browser
/// would — the grader sees the same tree a real DOM would build.
pub fn observe_html(html: &str, url: Option<String>) -> ObserveResult {
    let doc = scraper::Html::parse_document(html);
    let root = doc.root_element();
    let structure = element_to_probe(root);
    let title = doc
        .select(&scraper::Selector::parse("title").unwrap())
        .next()
        .map(|t| collapse_ws(&t.text().collect::<String>()))
        .filter(|s| !s.is_empty());
    ObserveResult {
        success: true,
        url,
        title,
        image: None, // static eye is structure-only; pixels need the browser eye
        structure: Some(structure),
        error: None,
    }
}

/// Recursively map a parsed element to a [`ProbeNode`], mirroring what a browser's
/// accessibility tree exposes: tag, implicit ARIA role, accessible name, direct text,
/// a curated attribute set, and element children in document order.
fn element_to_probe(el: scraper::ElementRef<'_>) -> ProbeNode {
    let ev = el.value();
    let tag = ev.name().to_ascii_lowercase();

    // Curated load-bearing attributes (mirrors the browser eye's `attrs`).
    let mut attrs: HashMap<String, String> = HashMap::new();
    for key in ["id", "class", "href", "type", "name", "aria-label", "alt", "role"] {
        if let Some(v) = ev.attr(key) {
            attrs.insert(key.to_string(), v.to_string());
        }
    }

    // Direct text: immediate text-node children only (this node's own text, not its
    // descendants') — matches ProbeNode.text semantics.
    let direct_text = {
        let s: String = el
            .children()
            .filter_map(|c| c.value().as_text().map(|t| t.to_string()))
            .collect::<Vec<_>>()
            .join(" ");
        let s = collapse_ws(&s);
        if s.is_empty() { None } else { Some(s) }
    };

    // Accessible name: explicit aria-label / alt wins; else the full descendant text
    // (how a browser computes an interactive element's name — covers
    // `<button><span>Submit</span></button>`); else the id. Role/tag gate the match in
    // `node_matches`, so a broad name never causes a FALSE match on its own.
    let name = attrs
        .get("aria-label")
        .cloned()
        .or_else(|| attrs.get("alt").cloned())
        .or_else(|| {
            let full = collapse_ws(&el.text().collect::<String>());
            if full.is_empty() { None } else { Some(full) }
        })
        .or_else(|| attrs.get("id").cloned());

    // Explicit role attr wins over the implicit mapping (author intent is authoritative).
    let role = attrs
        .get("role")
        .cloned()
        .or_else(|| implicit_role(&tag, &attrs));

    let children = el
        .children()
        .filter_map(scraper::ElementRef::wrap)
        .map(element_to_probe)
        .collect();

    ProbeNode {
        tag,
        role,
        name,
        text: direct_text,
        bounds: None, // no layout without a renderer; structural grading doesn't need it
        attrs: if attrs.is_empty() { None } else { Some(attrs) },
        children,
    }
}

/// Implicit ARIA role of an HTML element — the subset that matters for structural UI
/// grading. Faithful to the HTML-AAM mapping for the common interactive/landmark tags;
/// returns `None` for elements with no implicit role (a plain `<div>`/`<span>`), which
/// is correct — a role-constrained check simply won't match them.
fn implicit_role(tag: &str, attrs: &HashMap<String, String>) -> Option<String> {
    let role = match tag {
        "button" | "summary" => "button",
        "a" | "area" => {
            // Only a *linked* anchor has the link role.
            if attrs.contains_key("href") { "link" } else { return None }
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
        "nav" => "navigation",
        "header" => "banner",
        "footer" => "contentinfo",
        "main" => "main",
        "aside" => "complementary",
        "form" => "form",
        "img" => "img",
        "textarea" => "textbox",
        "select" => "combobox",
        "table" => "table",
        "ul" | "ol" => "list",
        "li" => "listitem",
        "input" => match attrs
            .get("type")
            .map(|s| s.to_ascii_lowercase())
            .as_deref()
            .unwrap_or("text")
        {
            "button" | "submit" | "reset" | "image" => "button",
            "checkbox" => "checkbox",
            "radio" => "radio",
            "range" => "slider",
            "search" => "searchbox",
            "email" | "tel" | "text" | "url" => "textbox",
            // `password` intentionally has NO ARIA role in the spec; leave it roleless
            // (it still matches on `tag: input`, which is how such checks are written).
            _ => return None,
        },
        _ => return None,
    };
    Some(role.to_string())
}

/// Collapse all runs of ASCII whitespace to single spaces and trim — so text/name
/// matching is insensitive to HTML source indentation.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perception::scoring::{grade_ui, UiCheck};

    fn check(tag: Option<&str>, role: Option<&str>, text: Option<&str>, min: u32) -> UiCheck {
        UiCheck {
            description: String::new(),
            tag: tag.map(str::to_string),
            role: role.map(str::to_string),
            text_contains: text.map(str::to_string),
            min_count: min,
        }
    }

    // what this catches: the headless grader must score a spec-perfect login page 3/3
    // from a pure parse (no browser eye-node) — the #206 false-zero regression. The HTML
    // is Asha's actual live render.
    #[test]
    fn login_page_grades_all_checks_headless() {
        let html = r#"<!DOCTYPE html><html><head><title>Login</title></head>
        <body>
          <h1>Sign in</h1>
          <form action="/submit" method="post">
            <label for="email">Email:</label>
            <input type="email" id="email" name="email" required>
            <label for="password">Password:</label>
            <input type="password" id="password" name="password" required>
            <button type="submit">Submit</button>
          </form>
        </body></html>"#;
        let obs = observe_html(html, None);
        assert!(obs.success);
        let checks = vec![
            check(Some("h1"), None, Some("sign in"), 1),
            check(Some("input"), None, None, 2),
            check(None, Some("button"), Some("submit"), 1),
        ];
        let grade = grade_ui(&obs, &checks, 1.0);
        assert!(grade.passed, "expected all 3 checks to pass, got: {}", grade.summary);
    }

    // what this catches: a <button> must carry the implicit ARIA role so a
    // `{role: button}` check matches — the exact criterion that scored 0 live.
    #[test]
    fn button_gets_implicit_role() {
        let obs = observe_html("<button>Go</button>", None);
        let root = obs.structure.unwrap();
        // find the button anywhere in the tree
        fn find<'a>(n: &'a ProbeNode, tag: &str) -> Option<&'a ProbeNode> {
            if n.tag == tag { return Some(n); }
            n.children.iter().find_map(|c| find(c, tag))
        }
        let btn = find(&root, "button").expect("button node present");
        assert_eq!(btn.role.as_deref(), Some("button"));
    }

    // what this catches: a missing/unbuilt artifact must score a MISS, never a false
    // pass — a benchmark that grades a page that isn't there is worthless.
    #[test]
    fn missing_file_is_a_miss_not_a_pass() {
        let obs = observe_file(Path::new("/nonexistent/never/index.html"));
        assert!(!obs.success);
        let checks = vec![check(Some("h1"), None, Some("sign in"), 1)];
        let grade = grade_ui(&obs, &checks, 1.0);
        assert!(!grade.passed);
    }
}
