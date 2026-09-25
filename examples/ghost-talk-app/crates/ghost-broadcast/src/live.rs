use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BroadcastFrame {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub encoded: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SinkFailure {
    pub sink: String,
    pub error: String,
}

pub trait BroadcastSink: Send {
    fn id(&self) -> &str;
    fn push(&mut self, frame: &BroadcastFrame) -> Result<(), String>;
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// One encoded input is fanned out to all sinks; individual sink failures are isolated.
pub struct BroadcastPipeline {
    sinks: BTreeMap<String, Box<dyn BroadcastSink>>,
    failures: Vec<SinkFailure>,
}
impl BroadcastPipeline {
    pub fn new() -> Self {
        Self {
            sinks: BTreeMap::new(),
            failures: Vec::new(),
        }
    }
    pub fn add_sink(&mut self, sink: Box<dyn BroadcastSink>) {
        self.sinks.insert(sink.id().to_owned(), sink);
    }
    pub fn remove_sink(&mut self, id: &str) -> bool {
        self.sinks.remove(id).is_some()
    }
    pub fn fan_out(&mut self, frame: &BroadcastFrame) {
        let mut failed = Vec::new();
        for (id, sink) in &mut self.sinks {
            if let Err(error) = sink.push(frame) {
                self.failures.push(SinkFailure {
                    sink: id.clone(),
                    error,
                });
                failed.push(id.clone());
            }
        }
        for id in failed {
            self.sinks.remove(&id);
        }
    }
    pub fn failures(&self) -> &[SinkFailure] {
        &self.failures
    }
    pub fn finish(&mut self) {
        for (id, sink) in &mut self.sinks {
            if let Err(error) = sink.finish() {
                self.failures.push(SinkFailure {
                    sink: id.clone(),
                    error,
                });
            }
        }
    }
}
impl Default for BroadcastPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct ProbeSink {
        id: String,
        fail: bool,
        count: Arc<Mutex<usize>>,
    }

    impl BroadcastSink for ProbeSink {
        fn id(&self) -> &str {
            &self.id
        }
        fn push(&mut self, _frame: &BroadcastFrame) -> Result<(), String> {
            *self.count.lock().unwrap() += 1;
            if self.fail {
                Err("sink failed".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn failing_sink_is_isolated_from_other_destinations() {
        let good_count = Arc::new(Mutex::new(0));
        let bad_count = Arc::new(Mutex::new(0));
        let mut pipeline = BroadcastPipeline::new();
        pipeline.add_sink(Box::new(ProbeSink {
            id: "good".into(),
            fail: false,
            count: good_count.clone(),
        }));
        pipeline.add_sink(Box::new(ProbeSink {
            id: "bad".into(),
            fail: true,
            count: bad_count.clone(),
        }));
        let frame = BroadcastFrame {
            sequence: 1,
            timestamp_ms: 1,
            encoded: vec![1],
        };
        pipeline.fan_out(&frame);
        pipeline.fan_out(&frame);
        assert_eq!(*good_count.lock().unwrap(), 2);
        assert_eq!(*bad_count.lock().unwrap(), 1);
        assert_eq!(pipeline.failures().len(), 1);
        assert_eq!(pipeline.failures()[0].sink, "bad");
    }
}
