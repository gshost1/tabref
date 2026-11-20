use serde_json::Value;

/// How much of a tracked reply's body has reached the page, against the
/// length its headers promise. Chrome withholds `loadingFinished` for some
/// replies, such as a `no-store` one whose body the page never reads, while
/// every byte is already in; counting `dataReceived` shows that nothing is
/// pending. A body sent with a content encoding is counted in encoded bytes,
/// which Chrome may report only at the end; a reply with no length waits for
/// `loadingFinished` as before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResponseBytes {
    expected: u64,
    encoded: bool,
    received: u64,
}

/// What a reply's status and headers say about the body still to come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplyBody {
    Empty,
    Counted(ResponseBytes),
    Unknown,
}

impl ReplyBody {
    pub(crate) fn of(params: &Value) -> Self {
        let response = params.get("response");
        let status = response
            .and_then(|response| response.get("status"))
            .and_then(Value::as_u64);
        if matches!(status, Some(204 | 304)) {
            return Self::Empty;
        }
        let headers = response
            .and_then(|response| response.get("headers"))
            .and_then(Value::as_object);
        let header = |name: &str| {
            headers.and_then(|headers| {
                headers
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .and_then(|(_, value)| value.as_str())
            })
        };
        let Some(expected) = header("content-length").and_then(|value| value.trim().parse().ok())
        else {
            return Self::Unknown;
        };
        if expected == 0 {
            return Self::Empty;
        }
        let encoded = header("content-encoding")
            .is_some_and(|value| !value.trim().eq_ignore_ascii_case("identity"));
        Self::Counted(ResponseBytes {
            expected,
            encoded,
            received: 0,
        })
    }
}

impl ResponseBytes {
    /// Adds one `dataReceived` event and returns whether the whole body has
    /// now arrived.
    pub(crate) fn add(&mut self, params: &Value) -> bool {
        let key = if self.encoded {
            "encodedDataLength"
        } else {
            "dataLength"
        };
        let length = params.get(key).and_then(Value::as_u64).unwrap_or(0);
        self.received = self.received.saturating_add(length);
        self.received >= self.expected
    }
}
