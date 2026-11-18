//! Resident vectors indexed by memory ID, with the number held in each embedding space.
//! Writers update both under the corpus write lock. A query checks its space once.
use crate::memory::types::MemoryEmbedding;
use continuum_store::data::vector_index::EmbedderId;
use std::collections::HashMap;

#[derive(Default)]
pub(in crate::memory) struct MemoryEmbeddings {
    rows: HashMap<String, MemoryEmbedding>,
    counts: HashMap<EmbedderId, usize>,
}

impl MemoryEmbeddings {
    pub(super) fn with_capacity(rows: usize) -> Self {
        Self {
            rows: HashMap::with_capacity(rows),
            counts: HashMap::new(),
        }
    }

    pub(super) fn from_rows(rows: HashMap<String, MemoryEmbedding>) -> Self {
        let mut counts = HashMap::new();
        for row in rows.values() {
            Self::count_row(&mut counts, &row.producer);
        }
        Self { rows, counts }
    }

    fn count_row(counts: &mut HashMap<EmbedderId, usize>, producer: &EmbedderId) {
        if let Some(count) = counts.get_mut(producer) {
            *count += 1;
        } else {
            counts.insert(producer.clone(), 1);
        }
    }

    pub(in crate::memory) fn accepts(&self, producer: &str) -> bool {
        self.counts.get(producer).copied().unwrap_or_default() == self.rows.len()
    }

    pub(in crate::memory) fn get(&self, id: &str) -> Option<&MemoryEmbedding> {
        self.rows.get(id)
    }

    pub(in crate::memory) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(in crate::memory) fn insert(&mut self, id: String, row: MemoryEmbedding) {
        self.remove(&id);
        Self::count_row(&mut self.counts, &row.producer);
        self.rows.insert(id, row);
    }

    pub(in crate::memory) fn remove(&mut self, id: &str) {
        if let Some(previous) = self.rows.remove(id) {
            if let std::collections::hash_map::Entry::Occupied(mut count) =
                self.counts.entry(previous.producer)
            {
                *count.get_mut() -= 1;
                if *count.get() == 0 {
                    count.remove();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_and_removing_the_last_foreign_vector_restores_the_space() {
        let vector = |name| MemoryEmbedding {
            producer: EmbedderId::new(name),
            values: vec![1.0],
        };
        let first = "first";
        let second = "second";
        let mut vectors = MemoryEmbeddings::from_rows(HashMap::from([
            (first.to_owned(), vector("a")),
            (second.to_owned(), vector("b")),
        ]));
        assert!(!vectors.accepts("a"));
        vectors.insert(second.to_owned(), vector("a"));
        assert!(vectors.accepts("a"));
        vectors.insert(first.to_owned(), vector("b"));
        assert!(!vectors.accepts("a"));
        vectors.remove(first);
        assert!(vectors.accepts("a"));
        vectors.remove(second);
        assert!(vectors.accepts("b"));
    }

    #[test]
    #[ignore = "10,003-row embedding space admission replay"]
    fn replay_space_check_during_replacement() {
        use std::{hint::black_box, time::Instant};
        let producer = EmbedderId::new("fixture");
        let rows = (0..10_003)
            .map(|i| {
                (
                    i.to_string(),
                    MemoryEmbedding {
                        values: vec![0.5; 768],
                        producer: producer.clone(),
                    },
                )
            })
            .collect();
        let mut vectors = MemoryEmbeddings::from_rows(rows);
        for mixed in [false, true, false] {
            let name = if mixed { "foreign" } else { producer.as_str() };
            vectors.insert(
                String::from("5000"),
                MemoryEmbedding {
                    values: vec![0.5; 768],
                    producer: EmbedderId::new(name),
                },
            );
            for indexed in [false, true] {
                let start = Instant::now();
                for _ in 0..128 {
                    let current = black_box(&vectors);
                    let accepted = if indexed {
                        current.accepts(producer.as_str())
                    } else {
                        current.rows.values().all(|row| row.producer == producer)
                    };
                    assert_eq!(accepted, !mixed);
                }
                println!(
                    "BASIS_CHECK mixed={mixed} indexed={indexed} checks=128 elapsed_us={}",
                    start.elapsed().as_micros()
                );
            }
        }
    }
}
