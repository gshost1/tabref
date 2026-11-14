/// How a browser endpoint was discovered. A `DevToolsActivePort` endpoint
/// may belong to a browser whose remote-debugging server asks its user to
/// approve each connection, so connecting to one waits for that approval and
/// proves it with `Browser.getVersion`; every other source is a server that
/// already answered or that tabref started itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointSource {
    JsonVersion,
    DevToolsActivePort,
    WebSocketUrl,
    Launched,
}
