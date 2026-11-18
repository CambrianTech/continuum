//! The chat boundary without a model: a content-only render, its parse and its refusals.

use super::*;

// what this catches: the parse boundary itself (shim linked, the render's native parser used, the
// string handed back and freed). Plain content under the content-only format comes back as content.
#[test]
fn content_only_output_parses_to_its_content() {
    let render = ChatRender::content_only(ReasoningFormat::None);
    let parsed = render.and_then(|render| render.parse("hello there", false));
    assert!(matches!(&parsed, Ok(json) if json.contains(r#""content":"hello there""#)), "{parsed:?}");
}

// what this catches: a streamed update re-sending the whole render (Codex on #6142). The same
// render parses an unfinished stream and then its completion, holding nothing but the handle.
#[test]
fn one_render_parses_each_streamed_update() {
    let Ok(render) = ChatRender::content_only(ReasoningFormat::Auto) else { panic!("content-only render refused") };
    let partial = render.parse("hel", true);
    let whole = render.parse("hello", false);
    assert!(matches!(&partial, Ok(json) if json.contains(r#""content":"hel""#)), "{partial:?}");
    assert!(matches!(&whole, Ok(json) if json.contains(r#""content":"hello""#)), "{whole:?}");
}

// what this catches: text with an interior NUL truncating what is parsed (it crosses by length).
#[test]
fn text_crosses_by_length_not_terminator() {
    let render = ChatRender::content_only(ReasoningFormat::None);
    let parsed = render.and_then(|render| render.parse("a\0b", false));
    assert!(matches!(&parsed, Ok(json) if json.contains(r#""content":"a\u0000b""#)), "{parsed:?}");
}
