//! Each owner suspends its children and returns their preservation results.
use std::future::Future;

/// Repeated suspension must preserve the same state. The owner retains custody on
/// cancellation and on refusal. A poll must yield promptly while I/O or GPU work is pending. Implementations can return a nested child report.
pub trait Lifecycle {
    type Suspension: Suspension;
    fn suspend(&mut self) -> impl Future<Output = Self::Suspension> + Send;
}

pub trait Suspension {
    fn is_preserved(&self) -> bool;
}

#[derive(Debug, PartialEq, Eq)]
pub enum Preservation<P, E> {
    Preserved,
    Pending(P),
    Failed(E),
}

impl<P, E> Suspension for Preservation<P, E> {
    fn is_preserved(&self) -> bool {
        matches!(self, Self::Preserved)
    }
}

// One declaration supplies the owned fields, traversal, and typed report shape.
macro_rules! persona_parts {
    ($($field:ident: $part:ident),+ $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct PersonaParts<$($part),+> {
            $(pub $field: $part),+
        }

        impl<$($part: Lifecycle + Send),+> Lifecycle for PersonaParts<$($part),+>
        where $($part::Suspension: Send),+
        {
            type Suspension = PersonaParts<$($part::Suspension),+>;
            async fn suspend(&mut self) -> Self::Suspension {
                let ($($field,)+) = futures::join!($(self.$field.suspend()),+);
                PersonaParts { $($field),+ }
            }
        }

        impl<$($part: Suspension),+> Suspension for PersonaParts<$($part),+> {
            fn is_preserved(&self) -> bool {
                $(self.$field.is_preserved())&&+
            }
        }
    };
}

persona_parts! {
    blocks: Blocks,
    tokens: Tokens,
    genome: Genome,
    learning: Learning,
    decode: Decode,
}
