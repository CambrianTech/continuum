//! The ONE route from the `clog_*` / `log_*` macros to the per-category log files.
//!
//! The macros are plain `tracing` events on [`CLOG_TARGET`]; they know nothing about the
//! file writer. This layer is the only thing that does: it picks those events off the
//! subscriber, resolves category and component (from the event's fields when the macro
//! named them, otherwise from the event's module path), and hands the line to a
//! [`ClogSink`]. In the core that sink is the LoggerModule's queue
//! (`modules::logger_clog_sink::LoggerQueueSink`), so the files, the
//! levels and the line format are exactly what the macros used to write directly.
//!
//! The fmt layer filters [`CLOG_TARGET`] out through [`fmt_layer_filter`], so a line goes
//! to its category file and never also into the core's firehose log.

use std::fmt::Write as _;

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::filter::{filter_fn, EnvFilter, FilterExt};
use tracing_subscriber::layer::{Context, Filter, Layer};

use super::{extract_component, module_path_to_category};

/// The target every `clog_*` / `log_*` event carries.
pub const CLOG_TARGET: &str = "continuum::clog";

/// Where routed lines land. The core's sink is the LoggerModule's queue
/// (`modules::logger_clog_sink::LoggerQueueSink`); tests use a fixture. The level is tracing's own,
/// so this module names nothing above the kernel.
pub trait ClogSink: Send + Sync + 'static {
    /// Is the sink accepting lines? Checked before the message is formatted, so an event
    /// fired before the sink exists costs no formatting.
    fn ready(&self) -> bool;
    fn write(&self, category: &str, level: Level, component: &str, message: &str);
}

pub struct ClogLayer<K: ClogSink> {
    sink: K,
}

impl<K: ClogSink> ClogLayer<K> {
    pub fn new(sink: K) -> Self {
        Self { sink }
    }
}

impl<S: Subscriber, K: ClogSink> Layer<S> for ClogLayer<K> {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        if meta.target() != CLOG_TARGET || !self.sink.ready() {
            return;
        }
        // Every callsite the tracing macros build carries `module_path!()`; an event with
        // none was not built by a clog macro and has nothing to route by.
        let Some(module_path) = meta.module_path() else {
            return;
        };
        let mut fields = ClogFields::default();
        event.record(&mut fields);
        let category = match fields.category.as_deref() {
            Some(named) => named,
            None => module_path_to_category(module_path), // clog_* names none: route by caller
        };
        let component = match fields.component.as_deref() {
            Some(named) => named,
            None => extract_component(module_path), // clog_* names none: the caller's module
        };
        self.sink.write(category, *meta.level(), component, &fields.message);
    }
}

/// The fmt layer's filter: the operator's `EnvFilter`, minus every clog event (those go to
/// their category file instead). Decided once per callsite, so it costs nothing per event.
pub fn fmt_layer_filter<S: Subscriber>(env_filter: EnvFilter) -> impl Filter<S> {
    filter_fn(|meta: &Metadata<'_>| meta.target() != CLOG_TARGET).and(env_filter)
}

#[derive(Default)]
struct ClogFields {
    category: Option<String>,
    component: Option<String>,
    message: String,
}

impl Visit for ClogFields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "category" => self.category = Some(value.to_owned()),
            "component" => self.component = Some(value.to_owned()),
            "message" => self.message.push_str(value),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // The macros record category/component with `%` and the message as
        // `format_args!`; both arrive here and their Debug rendering IS the text.
        let slot = match field.name() {
            "category" => self.category.get_or_insert_with(String::new),
            "component" => self.component.get_or_insert_with(String::new),
            "message" => &mut self.message,
            _ => return,
        };
        // Writing into a String cannot fail.
        let _ = write!(slot, "{value:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

    type Line = (String, Level, String, String);

    #[derive(Clone, Default)]
    struct FixtureSink(Arc<Mutex<Vec<Line>>>);

    impl ClogSink for FixtureSink {
        fn ready(&self) -> bool {
            true
        }
        fn write(&self, category: &str, level: Level, component: &str, message: &str) {
            self.0.lock().expect("fixture sink lock").push((
                category.to_owned(),
                level,
                component.to_owned(),
                message.to_owned(),
            ));
        }
    }

    #[derive(Clone, Default)]
    struct FmtCapture(Arc<Mutex<Vec<u8>>>);

    impl Write for FmtCapture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("fmt capture lock").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// what this catches: the macros as tracing events losing their file routing (wrong
    /// category, component, level or text), or reaching the fmt firehose as well as
    /// their category file — the "written twice" regression of the one-logging change.
    #[test]
    fn clog_and_log_macros_route_to_their_category_and_skip_the_fmt_layer() {
        let sink = FixtureSink::default();
        let fmt_out = FmtCapture::default();
        let writer = fmt_out.clone();
        let subscriber = tracing_subscriber::registry()
            .with(ClogLayer::new(sink.clone()))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(move || writer.clone())
                    .with_ansi(false)
                    .with_filter(fmt_layer_filter(EnvFilter::new("trace"))),
            );

        let user = "joel";
        tracing::subscriber::with_default(subscriber, || {
            crate::clog_info!("session started for {user}");
            crate::log_warn!("personas/helper/cognition", "DecisionEngine", "rate {}", 7);
            tracing::info!("an ordinary fmt line");
        });

        let lines = sink.0.lock().expect("fixture sink lock").clone();
        assert_eq!(
            lines,
            vec![
                (
                    module_path_to_category(module_path!()).to_owned(),
                    Level::INFO,
                    "tests".to_owned(),
                    "session started for joel".to_owned(),
                ),
                (
                    "personas/helper/cognition".to_owned(),
                    Level::WARN,
                    "DecisionEngine".to_owned(),
                    "rate 7".to_owned(),
                ),
            ]
        );
        let fmt_text = String::from_utf8(fmt_out.0.lock().expect("fmt capture lock").clone())
            .expect("fmt output is utf-8");
        assert!(fmt_text.contains("an ordinary fmt line"), "fmt layer saw: {fmt_text}");
        assert!(!fmt_text.contains("session started"), "clog_info! reached fmt: {fmt_text}");
        assert!(!fmt_text.contains("rate 7"), "log_warn! reached fmt: {fmt_text}");
    }
}
