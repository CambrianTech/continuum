//! The names a store failure reports: which collection, field, table, column or physical type.
//! Typed, so a caller matches on which name a failure is about, never a bare `String`; shared by
//! `Arc<str>`, so carrying one into an error copies a pointer, not the text.

macro_rules! store_name {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(std::sync::Arc<str>);

        impl $name {
            /// The name's text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(text: &str) -> Self {
                Self(text.into())
            }
        }

        impl From<&String> for $name {
            fn from(text: &String) -> Self {
                Self(text.as_str().into())
            }
        }

        impl From<String> for $name {
            fn from(text: String) -> Self {
                Self(text.into())
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                &*self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                &*self.0 == *other
            }
        }
    };
}

store_name!(
    /// A collection as the schema declares it (`chat_messages`).
    CollectionName
);
store_name!(
    /// A field of a collection as the schema declares it (`createdAt`).
    FieldName
);
store_name!(
    /// A table as the physical store names it.
    TableName
);
store_name!(
    /// A column as the physical store names it.
    ColumnName
);
store_name!(
    /// A physical storage type or class as the store reports it (`TEXT`, `INTEGER`).
    PhysicalType
);
