//! Source-local top-K selection without retaining a full native array.
use super::{CompiledFilter, RatingIndex, Selection, model::Candidate, native::Native};
use crate::{arguments::MediaType, error::ApiError};
use serde::{
    Deserializer as _,
    de::{SeqAccess, Visitor},
};
use serde_json::value::RawValue;
use std::{collections::BinaryHeap, fmt};

#[derive(Debug)]
struct Selector<'a, 'f> {
    source: &'a str,
    kind: MediaType,
    filter: &'f CompiledFilter,
    ratings: Option<&'a RatingIndex>,
}

pub(super) fn decode<'a>(
    source: &'a str,
    kind: MediaType,
    body: &'a [u8],
    filter: &CompiledFilter,
    ratings: Option<&'a RatingIndex>,
) -> Result<Selection<'a>, ApiError> {
    let mut decoder = serde_json::Deserializer::from_slice(body);
    let result = decoder.deserialize_seq(Selector {
        source,
        kind,
        filter,
        ratings,
    })?;

    decoder.end()?;
    result
}

impl<'de: 'a, 'a> Selector<'a, '_> {
    fn collect<A>(self, mut sequence: A) -> Result<Result<Selection<'a>, ApiError>, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let capacity = self.filter.capacity();
        let mut rows = BinaryHeap::with_capacity(capacity.min(128));
        let mut total = 0;
        let mut failure = None;

        while let Some(raw) = sequence.next_element::<&'de RawValue>()? {
            if failure.is_some() {
                continue;
            }

            match Native::decode(raw).and_then(|native| native.project(self.source, self.kind)) {
                Ok(mut item) => {
                    let ratings = self.ratings.map_or(&[][..], |index| index.for_item(&item));

                    item.enrich(ratings);

                    if !self.filter.matches(&item) {
                        continue;
                    }

                    let candidate =
                        Candidate::new(&item, self.filter, raw, total).with_ratings(ratings);

                    total += 1;
                    retain(&mut rows, candidate, capacity);
                }

                Err(error) => failure = Some(error),
            }
        }

        Ok(failure.map_or_else(
            || {
                Ok(Selection {
                    rows: rows.into_vec(),
                    total,
                    capacity,
                    order: self.filter.order(),
                })
            },
            Err,
        ))
    }
}

fn retain<'a>(rows: &mut BinaryHeap<Candidate<'a>>, candidate: Candidate<'a>, capacity: usize) {
    if rows.len() < capacity {
        rows.push(candidate);
    } else if let Some(mut largest) = rows.peek_mut()
        && candidate < *largest
    {
        *largest = candidate;
    }
}

impl<'de: 'a, 'a> Visitor<'de> for Selector<'a, '_> {
    type Value = Result<Selection<'a>, ApiError>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a native library array")
    }

    fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        self.collect(seq)
    }
}
