use std::{marker::PhantomData, sync::RwLock};

use serde::{de::Visitor, ser::SerializeSeq, Deserialize, Serialize};

use super::{Shard, ShardList};

struct ShardVisitor<T> {
    marker: PhantomData<T>,
}

impl<T> ShardVisitor<T> {
    fn new() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<'de, T: Deserialize<'de>> Visitor<'de> for ShardVisitor<T> {
    type Value = Vec<RwLock<Shard<T>>>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a shard vector")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));

        while let Some(e) = seq.next_element()? {
            vec.push(RwLock::new(e));
        }

        Ok(vec)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for ShardList<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let shards = deserializer.deserialize_seq(ShardVisitor::new())?;
        Ok(ShardList(shards))
    }
}

impl<T: Serialize> Serialize for ShardList<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for s in self.iter() {
            seq.serialize_element(&*s.read().unwrap())?;
        }

        seq.end()
    }
}
