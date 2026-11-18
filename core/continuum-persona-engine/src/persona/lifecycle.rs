//! The one lifecycle every owner implements: each owner suspends or releases its children and
//! returns their typed reports, nested. A parent's operation is its children's, run together
//! (independent children never wait on each other), and a pending or failed child reaches the root.
use std::future::Future;

/// Repeated suspension must preserve the same state, and repeated release must free nothing twice.
/// The owner retains custody on cancellation and on refusal. A poll must yield promptly while I/O
/// or GPU work is pending. Implementations can return a nested child report.
pub trait Lifecycle {
    type Suspension: Report;
    type Release: Report;
    /// Keep the state, moved where it can wait (her KV to RAM, her shadow and optimizer kept).
    fn suspend(&mut self) -> impl Future<Output = Self::Suspension> + Send;
    /// Free what the state holds, complete only when its owner proves the memory released.
    fn release(&mut self) -> impl Future<Output = Self::Release> + Send;
}

/// Whether an operation finished for this node and everything under it.
pub trait Report {
    fn is_complete(&self) -> bool;
}

/// How a suspension ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Preservation<P, E> {
    Preserved,
    Pending(P),
    Failed(E),
}

impl<P, E> Report for Preservation<P, E> {
    fn is_complete(&self) -> bool {
        matches!(self, Self::Preserved)
    }
}

/// How a release ended: `Released` only once the memory is proven free.
#[derive(Debug, PartialEq, Eq)]
pub enum Releasing<P, E> {
    Released,
    Pending(P),
    Failed(E),
}

impl<P, E> Report for Releasing<P, E> {
    fn is_complete(&self) -> bool {
        matches!(self, Self::Released)
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
        where $($part::Suspension: Send, $part::Release: Send),+
        {
            type Suspension = PersonaParts<$($part::Suspension),+>;
            type Release = PersonaParts<$($part::Release),+>;
            async fn suspend(&mut self) -> Self::Suspension {
                let ($($field,)+) = futures::join!($(self.$field.suspend()),+);
                PersonaParts { $($field),+ }
            }
            async fn release(&mut self) -> Self::Release {
                let ($($field,)+) = futures::join!($(self.$field.release()),+);
                PersonaParts { $($field),+ }
            }
        }

        impl<$($part: Report),+> Report for PersonaParts<$($part),+> {
            fn is_complete(&self) -> bool {
                $(self.$field.is_complete())&&+
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
