use crate::executor::spawn;
use crate::log::{debug, error};
use crate::state_machine::prelude::*;
use async_trait::async_trait;
use futures::channel::mpsc::{self, SendError, TrySendError};
use futures::channel::oneshot;
use futures::{FutureExt, SinkExt, Stream, StreamExt, TryStreamExt};
use serde_json::{self as json, Value as Json};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use wasm_bindgen::closure::WasmClosure;
use wasm_bindgen::convert::FromWasmAbi;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

const NORMAL_CLOSURE_CODE: u16 = 1000;
const ABNORMAL_CLOSURE_CODE: u16 = 1006;

pub type ConnIdx = usize;

pub type WsOutgoingReceiver = mpsc::Receiver<Json>;
pub type WsIncomingSender = mpsc::Sender<(ConnIdx, WebSocketEvent)>;

type WsTransportReceiver = mpsc::Receiver<WsTransportEvent>;
type WsTransportSender = mpsc::Sender<WsTransportEvent>;

type IncomingShutdownTx = oneshot::Sender<()>;
type OutgoingShutdownTx = mpsc::Sender<()>;
type ShutdownRx = oneshot::Receiver<()>;

type TransportClosure = Closure<dyn FnMut(JsValue)>;

/// This is just an alias of the `Future<Output = ()> + Send + Unpin + 'static` trait.
/// Unfortunately, the trait type alias is an [unstable feature](https://github.com/rust-lang/rust/issues/41517).
trait ShutdownFut: Future<Output = ()> + Send + Unpin + 'static {}
impl<F: Future<Output = ()> + Send + Unpin + 'static> ShutdownFut for F {}

/// The `WsEventReceiver` wrapper that filters and maps the incoming `WebSocketEvent` events into `Result<Json, WebSocketError>`.
pub struct WsIncomingReceiver {
    inner: WsEventReceiver,
    closed: bool,
}

impl Stream for WsIncomingReceiver {
    type Item = Result<Json, WebSocketError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.closed {
            error!("Attempted to poll WsIncomingReceiver after completion");
            return Poll::Ready(None);
        }
        let event = match Stream::poll_next(Pin::new(&mut self.inner), cx) {
            Poll::Ready(Some((_conn_idx, event))) => event,
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Pending => return Poll::Pending,
        };
        match event {
            WebSocketEvent::Establish => {
                error!("WsIncomingReceiver is expected to be established already");
                Poll::Pending
            },
            WebSocketEvent::Closing { .. } | WebSocketEvent::Closed { .. } => {
                self.closed = true;
                Poll::Ready(None)
            },
            WebSocketEvent::Error(error) => Poll::Ready(Some(Err(error))),
            WebSocketEvent::Incoming(incoming) => Poll::Ready(Some(Ok(incoming))),
        }
    }
}

#[derive(Debug)]
pub struct WsEventReceiver {
    inner: mpsc::Receiver<(ConnIdx, WebSocketEvent)>,
    /// Is used to determine when the receiver is dropped.
    shutdown_tx: IncomingShutdownTx,
}

impl Unpin for WsEventReceiver {}

impl Stream for WsEventReceiver {
    type Item = (ConnIdx, WebSocketEvent);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Stream::poll_next(Pin::new(&mut self.inner), cx)
    }
}

#[derive(Debug, Clone)]
pub struct WsOutgoingSender {
    inner: mpsc::Sender<Json>,
    /// Is used to determine when all senders are dropped.
    shutdown_tx: OutgoingShutdownTx,
}

/// Consider implementing the `Sink` trait.
/// Please note `WsOutgoingSender` must not provide a way to close the [`WsOutgoingSender::inner`] channel,
/// because the shutdown_tx wouldn't be closed properly.
impl WsOutgoingSender {
    pub async fn send(&mut self, msg: Json) -> Result<(), SendError> { self.inner.send(msg).await }

    pub fn try_send(&mut self, msg: Json) -> Result<(), TrySendError<Json>> { self.inner.try_send(msg) }
}

#[derive(Debug)]
pub enum WebSocketEvent {
    /// A WebSocket connection is established.
    Establish,
    /// A WebSocket connection is being closing and it should not be used anymore.
    Closing { reason: ClosureReason },
    /// A WebSocket connection has been closed.
    Closed { reason: ClosureReason },
    /// An error has occurred.
    /// Please note some of the errors lead to the connection close.
    Error(WebSocketError),
    /// A message has been received through a WebSocket connection.
    Incoming(Json),
}

#[derive(Debug)]
pub enum WebSocketError {
    OutgoingError { reason: OutgoingError, outgoing: Json },
    UnderlyingError { description: String },
    InvalidIncoming { description: String },
}

#[derive(Debug)]
pub enum OutgoingError {
    IsNotConnected,
    SerializingError(String),
}

/// The [status codes](https://datatracker.ietf.org/doc/html/rfc6455#section-7.4.1) representation.
#[derive(Clone, Debug, PartialEq)]
pub enum ClosureReason {
    /// 1000 indicates a normal closure, meaning that the purpose for
    /// which the connection was established has been fulfilled.
    NormalClosure,
    /// 1001 indicates that an endpoint is "going away", such as a server
    /// going down or a browser having navigated away from a page.
    ///
    /// 1011 indicates that a server is terminating the connection because
    /// it encountered an unexpected condition that prevented it from
    /// fulfilling the request.
    GoingAway,
    /// 1006 is a reserved value and MUST NOT be set as a status code in a
    /// Close control frame by an endpoint.  It is designated for use in
    /// applications expecting a status code to indicate that the
    /// connection was closed abnormally, e.g., without sending or
    /// receiving a Close control frame.
    ClientReachedUnexpectedState,
    /// 1002, 1003, 1007, 1008, 1009, 1010
    ProtocolError,
    /// 1015 indicates that the connection was closed due to a failure to perform a TLS handshake
    /// (e.g., the server certificate can't be verified).
    TlsError,
    /// The client closed on a `WsTransportError` error.
    ClientClosedOnUnderlyingError(String),
    Other(u16),
}

impl ClosureReason {
    // https://datatracker.ietf.org/doc/html/rfc6455#section-7.4.1
    fn from_status_code(code: u16) -> ClosureReason {
        match code {
            1000 => ClosureReason::NormalClosure,
            1001 | 1011 => ClosureReason::GoingAway,
            1002 | 1003 | 1007 | 1008 | 1009 | 1010 => ClosureReason::ProtocolError,
            1015 => ClosureReason::TlsError,
            code => ClosureReason::Other(code),
        }
    }
}

// TODO change the error type
pub fn spawn_ws_transport(idx: ConnIdx, url: &str) -> Result<(WsOutgoingSender, WsEventReceiver), String> {
    let (ws, closures, ws_transport_rx) = init_ws(url)?;
    let (incoming_tx, incoming_rx, incoming_shutdown) = incoming_channel(1024);
    let (outgoing_tx, outgoing_rx, outgoing_shutdown) = outgoing_channel(1024);

    let user_shutdown = into_one_shutdown(incoming_shutdown, outgoing_shutdown);

    let state_event_rx = StateEventListener::new(outgoing_rx, ws_transport_rx, user_shutdown);
    let ws_ctx = WsContext {
        idx,
        ws,
        event_tx: incoming_tx,
        state_event_rx,
    };

    let fut = async move {
        let state_machine: StateMachine<_, ()> = StateMachine::from_ctx(ws_ctx);
        state_machine.run(ConnectingState).await;
        // do any action to move the `closures` into this async block to keep it alive until the `state_machine` finishes
        drop(closures);
    };
    spawn(fut);

    Ok((outgoing_tx, incoming_rx))
}

pub async fn ws_transport(idx: ConnIdx, url: &str) -> Result<(WsOutgoingSender, WsIncomingReceiver), String> {
    let (sender, mut receiver) = spawn_ws_transport(idx, url)?;
    while let Some((_conn_idx, event)) = receiver.next().await {
        match event {
            WebSocketEvent::Establish => break,
            WebSocketEvent::Closed { reason } | WebSocketEvent::Closing { reason } => {
                return ERR!("Couldn't connect to {}: {:?}", url, reason)
            },
            // if the error is an underlying error, the connection will close immediately
            WebSocketEvent::Error(e) => error!("{:?}", e),
            WebSocketEvent::Incoming(incoming) => error!(
                "Unexpected incoming while the connection is not established: {:?}",
                incoming
            ),
        }
    }
    // here we have the established connection
    let receiver = WsIncomingReceiver {
        inner: receiver,
        closed: false,
    };
    Ok((sender, receiver))
}

fn incoming_channel(capacity: usize) -> (WsIncomingSender, WsEventReceiver, impl ShutdownFut) {
    let (event_tx, event_rx) = mpsc::channel(capacity);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let incoming_rx = WsEventReceiver {
        inner: event_rx,
        shutdown_tx,
    };

    // convert the `oneshot::Receiver<()>` into `impl Future<Output=()>`
    let shutdown_rx = shutdown_rx.map(|_| ());
    (event_tx, incoming_rx, shutdown_rx)
}

fn outgoing_channel(capacity: usize) -> (WsOutgoingSender, WsOutgoingReceiver, impl ShutdownFut) {
    let (outgoing_tx, outgoing_rx) = mpsc::channel(capacity);
    let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
    let outgoing_tx = WsOutgoingSender {
        inner: outgoing_tx,
        shutdown_tx,
    };

    // convert the `mpsc::Receiver<()>` into `impl Future<Output=()>`
    let shutdown_rx = shutdown_rx.collect::<Vec<_>>().map(|_| ());
    (outgoing_tx, outgoing_rx, shutdown_rx)
}

fn into_one_shutdown(left: impl ShutdownFut, right: impl ShutdownFut) -> ShutdownRx {
    use futures::future::{select, Either};

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let fut = async move {
        match select(left, right).await {
            Either::Left((_left_output, right_fut)) => right_fut.await,
            Either::Right((_right_output, left_fut)) => left_fut.await,
        }
        drop(shutdown_tx);
    };

    spawn(fut);
    shutdown_rx
}

/// The JS closures that have to be alive until the corresponding WebSocket exists.
struct WsClosures {
    onopen_closure: TransportClosure,
    onclose_closure: TransportClosure,
    onerror_closure: TransportClosure,
    onmessage_closure: TransportClosure,
}

/// Although wasm is currently single-threaded, we can implement the `Send` trait for `WsClosures`,
/// but it won't be safe when wasm becomes multi-threaded.
unsafe impl Send for WsClosures {}

fn init_ws(url: &str) -> Result<(WebSocket, WsClosures, WsTransportReceiver), String> {
    // TODO figure out how to extract an error description without stack trace
    let ws = WebSocket::new(url).map_err(|e| format!("{:?}", e))?;

    let (tx, rx) = mpsc::channel(1024);

    let onopen_closure = construct_ws_event_closure(|_: JsValue| WsTransportEvent::Establish, tx.clone());
    let onclose_closure = construct_ws_event_closure(|close: CloseEvent| WsTransportEvent::from(close), tx.clone());
    let onerror_closure = construct_ws_event_closure(|error: ErrorEvent| WsTransportEvent::from(error), tx.clone());
    let onmessage_closure = construct_ws_event_closure(
        |message: MessageEvent| match decode_incoming(message) {
            Ok(response) => WsTransportEvent::Incoming(response),
            Err(e) => WsTransportEvent::Error(e),
        },
        tx.clone(),
    );

    ws.set_onopen(Some(onopen_closure.as_ref().unchecked_ref()));
    ws.set_onclose(Some(onclose_closure.as_ref().unchecked_ref()));
    ws.set_onerror(Some(onerror_closure.as_ref().unchecked_ref()));
    ws.set_onmessage(Some(onmessage_closure.as_ref().unchecked_ref()));

    // keep the closures in the memory until the `ws` exists
    let closures = WsClosures {
        onopen_closure,
        onclose_closure,
        onerror_closure,
        onmessage_closure,
    };

    Ok((ws, closures, rx))
}

struct WsContext {
    idx: ConnIdx,
    ws: WebSocket,
    /// The sender used to send the transport events outside (to the userspace).
    event_tx: WsIncomingSender,
    /// The stream of internal events that may come from either WebSocket transport or outside (userspace, such as outgoing messages).
    state_event_rx: StateEventListener,
}

impl WsContext {
    fn send_to_ws(&self, outgoing: Json) -> Result<(), WebSocketError> {
        match json::to_string(&outgoing) {
            Ok(req) => self.ws.send_with_str(&req).map_err(|error| {
                let description = format!("{:?}", error);
                WebSocketError::UnderlyingError { description }
            }),
            Err(e) => {
                let reason = OutgoingError::SerializingError(e.to_string());
                Err(WebSocketError::OutgoingError { reason, outgoing })
            },
        }
    }

    /// Send the `event` to the corresponding `WebSocketReceiver` instance.
    fn notify_listener(&mut self, event: WebSocketEvent) {
        if !self.event_tx.is_closed() {
            if let Err(e) = self.event_tx.try_send((self.idx, event)) {
                let error = e.to_string();
                let event = e.into_inner();
                error!("Error sending WebSocketEvent {:?}: {}", event, error);
            }
        }
    }

    fn send_unexpected_outgoing_back(&mut self, outgoing: Json, current_state: &str) {
        error!(
            "Unexpected outgoing message while the socket idx={} state is {}",
            self.idx, current_state
        );
        let error = WebSocketEvent::Error(WebSocketError::OutgoingError {
            reason: OutgoingError::IsNotConnected,
            outgoing,
        });
        self.notify_listener(error);
    }

    fn notify_about_underlying_err(&mut self, description: String) {
        let error = WebSocketEvent::Error(WebSocketError::UnderlyingError { description });
        self.notify_listener(error);
    }

    fn close_ws(&self, closure_code: u16) {
        if let Err(e) = self.ws.close_with_code(closure_code) {
            // TODO figure out how to extract an error description without stack trace
            error!("Unexpected error when closing WebSocket: {:?}", e);
        }
    }
}

/// `WsContext` is not thread-safety `Send` because [`WebSocket::ws`] is not `Send` by default.
/// Although wasm is currently single-threaded, we can implement the `Send` trait for `WsContext`,
/// but it won't be safe when wasm becomes multi-threaded.
unsafe impl Send for WsContext {}

struct StateEventListener {
    rx: Box<dyn Stream<Item = StateEvent> + Unpin + Send>,
}

impl StateEventListener {
    /// Combine the `outgoing_stream` and `ws_stream` into one stream of the internal events.
    /// `ws_stream` - is a stream of the `WebSocket` events.
    /// `outgoing_stream` - is a stream of the outgoing messages came from outside (userspace).
    fn new(outgoing_stream: WsOutgoingReceiver, ws_stream: WsTransportReceiver, shutdown_rx: ShutdownRx) -> Self {
        use futures::stream::select;

        let mapperd_outgoing = outgoing_stream.map(StateEvent::OutgoingMessage);
        let mapped_ws_transport = ws_stream.map(StateEvent::WsTransportEvent);
        let mapped_shutdown = shutdown_rx.map(|_| StateEvent::UserSideClosed).into_stream();

        // combine the streams into one
        let internal_stream = select(select(mapperd_outgoing, mapped_ws_transport), mapped_shutdown);
        StateEventListener {
            rx: Box::new(internal_stream),
        }
    }

    async fn receive_one(&mut self) -> Option<StateEvent> { self.rx.next().await }
}

/// The combination of `WsTransportEvent` and `OutgoingEvent`
/// obtained by merging `WsTransportReceiver` and `WsOutgoingReceiver` listeners.
#[derive(Debug)]
enum StateEvent {
    /// All instances of `WsOutgoingSender` and `WsIncomingReceiver` were dropped.
    UserSideClosed,
    /// Received an outgoing message. It should be forwarded to `WebSocket`.
    OutgoingMessage(Json),
    /// Received a `WsTransportEvent` event. It might be an incoming message from `WebSocket` or something else.
    WsTransportEvent(WsTransportEvent),
}

#[derive(Debug)]
enum WsTransportEvent {
    Establish,
    Close {
        /// https://datatracker.ietf.org/doc/html/rfc6455#section-7.4.1
        code: u16,
    },
    Error(String),
    Incoming(Json),
}

impl From<CloseEvent> for WsTransportEvent {
    fn from(close: CloseEvent) -> Self { WsTransportEvent::Close { code: close.code() } }
}

impl From<ErrorEvent> for WsTransportEvent {
    fn from(error: ErrorEvent) -> Self { WsTransportEvent::Error(error.message()) }
}

struct ConnectingState;
struct OpenState;
struct ClosingState {
    reason: ClosureReason,
}
struct ClosedState {
    reason: ClosureReason,
}

impl TransitionFrom<ConnectingState> for OpenState {}
impl TransitionFrom<ConnectingState> for ClosingState {}
impl TransitionFrom<ConnectingState> for ClosedState {}
impl TransitionFrom<OpenState> for ClosingState {}
impl TransitionFrom<OpenState> for ClosedState {}
impl TransitionFrom<ClosingState> for ClosedState {}

#[async_trait]
impl LastState for ClosedState {
    type Ctx = WsContext;
    type Result = ();

    async fn on_changed(self: Box<Self>, ctx: &mut Self::Ctx) -> Self::Result {
        debug!("WebSocket idx={} => ClosedState", ctx.idx);
        ctx.notify_listener(WebSocketEvent::Closed { reason: self.reason })
    }
}

#[async_trait]
impl State for ConnectingState {
    type Ctx = WsContext;
    type Result = ();

    async fn on_changed(self: Box<Self>, ctx: &mut Self::Ctx) -> StateResult<Self::Ctx, Self::Result> {
        debug!("WebSocket idx={} => ConnectingState", ctx.idx);
        while let Some(event) = ctx.state_event_rx.receive_one().await {
            match event {
                // there is no need to keep the connection, so close the socket and change the state into `ClosingState`
                StateEvent::UserSideClosed => return Self::change_state(ClosingState::normal_closure()),
                StateEvent::OutgoingMessage(outgoing) => ctx.send_unexpected_outgoing_back(outgoing, "ConnectingState"),
                StateEvent::WsTransportEvent(WsTransportEvent::Establish) => return Self::change_state(OpenState),
                StateEvent::WsTransportEvent(WsTransportEvent::Close { code }) => {
                    return Self::change_state(ClosedState::from_status_code(code))
                },
                StateEvent::WsTransportEvent(WsTransportEvent::Error(error)) => {
                    ctx.notify_about_underlying_err(error.clone());
                    // if an underlying error has occurred, it's better to close the socket
                    return Self::change_state(ClosingState::from_underlying_error(error));
                },
                StateEvent::WsTransportEvent(WsTransportEvent::Incoming(incoming)) => error!(
                    "Unexpected incoming message {} while the socket idx={} state is ConnectingState",
                    ctx.idx, incoming
                ),
            }
        }
        error!("StateEventListener is closed unexpectedly");
        ctx.close_ws(ABNORMAL_CLOSURE_CODE);
        Self::change_state(ClosedState::unexpected_closure())
    }
}

#[async_trait]
impl State for OpenState {
    type Ctx = WsContext;
    type Result = ();

    async fn on_changed(self: Box<Self>, ctx: &mut Self::Ctx) -> StateResult<Self::Ctx, Self::Result> {
        debug!("WebSocket idx={} => OpenState", ctx.idx);
        // notify the listener about the changed state
        ctx.notify_listener(WebSocketEvent::Establish);

        // wait for the `WsTransportEvent::Established` event or another one
        while let Some(event) = ctx.state_event_rx.receive_one().await {
            match event {
                // there is no need to keep the connection, so close the socket and change the state into `ClosingState`
                StateEvent::UserSideClosed => return Self::change_state(ClosingState::normal_closure()),
                StateEvent::OutgoingMessage(outgoing) => {
                    if let Err(e) = ctx.send_to_ws(outgoing) {
                        error!("{:?}", e);
                        ctx.notify_listener(WebSocketEvent::Error(e));
                    }
                },
                StateEvent::WsTransportEvent(WsTransportEvent::Establish) => {
                    error!("Unexpected WsTransport::Establish event")
                },
                StateEvent::WsTransportEvent(WsTransportEvent::Close { code }) => {
                    return Self::change_state(ClosedState::from_status_code(code))
                },
                StateEvent::WsTransportEvent(WsTransportEvent::Error(error)) => {
                    ctx.notify_about_underlying_err(error.clone());
                    // if an underlying error has occurred, it's better to close the socket
                    return Self::change_state(ClosingState::from_underlying_error(error));
                },
                StateEvent::WsTransportEvent(WsTransportEvent::Incoming(incoming)) => {
                    ctx.notify_listener(WebSocketEvent::Incoming(incoming))
                },
            }
        }

        error!("StateEventListener is closed unexpectedly");
        ctx.close_ws(ABNORMAL_CLOSURE_CODE);
        Self::change_state(ClosedState::unexpected_closure())
    }
}

#[async_trait]
impl State for ClosingState {
    type Ctx = WsContext;
    type Result = ();

    async fn on_changed(self: Box<Self>, ctx: &mut Self::Ctx) -> StateResult<Self::Ctx, Self::Result> {
        debug!("WebScoket idx={} => ClosingState", ctx.idx);
        // notify the listener about the changed state to prevent new outgoing messages
        ctx.notify_listener(WebSocketEvent::Closing {
            reason: self.reason.clone(),
        });
        ctx.close_ws(NORMAL_CLOSURE_CODE);

        // wait for the `WsTransportEvent::Close` event or another one
        while let Some(event) = ctx.state_event_rx.receive_one().await {
            match event {
                StateEvent::UserSideClosed => (), // ignore this event because we are waiting for the connection to close already
                StateEvent::OutgoingMessage(outgoing) => ctx.send_unexpected_outgoing_back(outgoing, "ClosingState"),
                // it doesn't matter with which status code the transport was closed because we already have an actual [`Self::reason`]
                StateEvent::WsTransportEvent(WsTransportEvent::Close { .. }) => {
                    return Self::change_state(ClosedState::from_reason(self.reason))
                },
                StateEvent::WsTransportEvent(WsTransportEvent::Error(error)) => ctx.notify_about_underlying_err(error),
                StateEvent::WsTransportEvent(event) => error!("Unexpected WsTransportEvent: {:?}", event),
            }
        }

        error!("StateEventListener is closed unexpectedly");
        Self::change_state(ClosedState::unexpected_closure())
    }
}

impl ClosingState {
    fn normal_closure() -> ClosingState {
        ClosingState {
            reason: ClosureReason::NormalClosure,
        }
    }

    fn from_underlying_error(error: String) -> ClosingState {
        ClosingState {
            reason: ClosureReason::ClientClosedOnUnderlyingError(error),
        }
    }

    fn from_status_code(code: u16) -> ClosingState {
        ClosingState {
            reason: ClosureReason::from_status_code(code),
        }
    }
}

impl ClosedState {
    fn unexpected_closure() -> ClosedState {
        ClosedState {
            reason: ClosureReason::ClientReachedUnexpectedState,
        }
    }

    fn from_status_code(code: u16) -> ClosedState {
        ClosedState {
            reason: ClosureReason::from_status_code(code),
        }
    }

    fn from_reason(reason: ClosureReason) -> ClosedState { ClosedState { reason } }
}

fn decode_incoming(incoming: MessageEvent) -> Result<Json, String> {
    match incoming.data().dyn_into::<js_sys::JsString>() {
        Ok(txt) => {
            // todo measure
            let txt = String::from(txt);
            json::from_str(&txt).map_err(|e| format!("Error deserializing an incoming payload: {}", e))
        },
        Err(e) => Err(format!("Unknown MessageEvent {:?}", e)),
    }
}

/// Please note the `Event` type can be `JsValue`. It doesn't lead to a runtime error, because [`JsValue::dyn_into<JsValue>()`] returns itself.
fn construct_ws_event_closure<Event, F>(mut f: F, mut event_tx: WsTransportSender) -> Closure<dyn FnMut(JsValue)>
where
    F: FnMut(Event) -> WsTransportEvent + 'static,
    Event: JsCast + 'static,
{
    Closure::new(move |event: JsValue| {
        let transport_event = match event.dyn_into::<Event>() {
            Ok(event) => f(event),
            // the `Err` variant contains untouched source `JsValue`
            Err(e) => {
                // consider using another way to obtain the `Event` type name
                let expected = std::any::type_name::<Event>();
                let error_desc = format!("Expected {}, found: {:?}", expected, e);
                WsTransportEvent::Error(error_desc)
            },
        };
        if let Err(e) = event_tx.try_send(transport_event) {
            let error = e.to_string();
            let event = e.into_inner();
            error!("Error sending WebSocketEvent {:?}: {}", event, error);
        }
    })
}

mod tests {
    use super::*;
    use crate::block_on;
    use crate::executor::Timer;
    use crate::for_tests::register_wasm_log;
    use crate::log::LogLevel;
    use futures::future::{select, Either};
    use futures::SinkExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    lazy_static! {
        static ref CONN_IDX: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    }

    async fn wait_for_event<Rx>(rx: &mut Rx, seconds: f64) -> Option<(ConnIdx, WebSocketEvent)>
    where
        Rx: Stream<Item = (ConnIdx, WebSocketEvent)> + Unpin,
    {
        let fut = select(rx.next(), Timer::sleep(seconds));
        match fut.await {
            Either::Left((event, _timer)) => event,
            Either::Right(_) => panic!("Timeout expired waiting for a transport event"),
        }
    }

    async fn wait_for_nothing<Rx>(rx: &mut Rx, seconds: f64)
    where
        Rx: Stream<Item = (ConnIdx, WebSocketEvent)> + Unpin,
    {
        let fut = select(rx.next(), Timer::sleep(seconds));
        match fut.await {
            Either::Left((event, _timer)) => panic!(
                "Expected no transport events for {} seconds, found: {:?}",
                seconds, event
            ),
            Either::Right(_) => (),
        }
    }

    #[wasm_bindgen_test]
    async fn test_websocket() {
        register_wasm_log(LogLevel::Debug);
        let conn_idx = CONN_IDX.fetch_add(1, Ordering::Relaxed);

        let (mut outgoing_tx, mut incoming_rx) =
            spawn_ws_transport(conn_idx, "wss://electrum1.cipig.net:30017").expect("!spawn_ws_transport");

        match wait_for_event(&mut incoming_rx, 5.).await {
            Some((_conn_idx, WebSocketEvent::Establish)) => (),
            other => panic!("Expected 'Establish' event, found: {:?}", other),
        }

        let get_version = json!({
            "jsonrpc": "2.0",
            "id": "0",
            "method": "server.version",
            "params": ["1.2", "1.4"],
        });
        outgoing_tx.send(get_version).await.expect("!outgoing_tx.send");

        match wait_for_event(&mut incoming_rx, 5.).await {
            Some((_conn_idx, WebSocketEvent::Incoming(response))) => {
                debug!("Response: {:?}", response);
                assert!(response.get("result").is_some());
            },
            other => panic!("Expected 'Incoming' event, found: {:?}", other),
        }

        drop(outgoing_tx);
        // Even if the `WsOutgoingSender` is closed, the transport must not close.
        wait_for_nothing(&mut incoming_rx, 1.).await;

        // It's possible for `wasm_ws` submodules ONLY.
        // Generally, the shutdown channel has to close on the `WsIncomingReceiver` instance drop.
        incoming_rx
            .shutdown_tx
            .send(())
            .expect("shutdown_rx must not be dropped");
        let mut incoming_rx = incoming_rx.inner;

        match wait_for_event(&mut incoming_rx, 0.5).await {
            Some((_conn_idx, WebSocketEvent::Closing { reason })) if reason == ClosureReason::NormalClosure => (),
            other => panic!(
                "Expected 'Closing' event with 'ClientClosed' reason, found: {:?}",
                other
            ),
        }
        match wait_for_event(&mut incoming_rx, 0.5).await {
            Some((_conn_idx, WebSocketEvent::Closed { reason })) if reason == ClosureReason::NormalClosure => (),
            other => panic!("Expected 'Closed' event with 'ClientClosed' reason, found: {:?}", other),
        }
    }

    #[wasm_bindgen_test]
    async fn test_websocket_unreachable_url() {
        register_wasm_log(LogLevel::Debug);
        let conn_idx = CONN_IDX.fetch_add(1, Ordering::Relaxed);

        // TODO check if outgoing messages are ignored non-open states
        let (_outgoing_tx, mut incoming_rx) =
            spawn_ws_transport(conn_idx, "ws://electrum1.cipig.net:10017").expect("!spawn_ws_transport");

        match wait_for_event(&mut incoming_rx, 5.).await {
            Some((_conn_idx, WebSocketEvent::Error(WebSocketError::UnderlyingError { .. }))) => (),
            other => panic!("Expected 'UnderlyingError', found: {:?}", other),
        }
        match wait_for_event(&mut incoming_rx, 0.5).await {
            Some((
                _conn_idx,
                WebSocketEvent::Closing {
                    reason: _reason @ ClosureReason::ClientClosedOnUnderlyingError(_),
                },
            )) => (),
            other => panic!(
                "Expected 'Closing' event with 'ClosedOnUnderlyingError' reason, found: {:?}",
                other
            ),
        }
        match wait_for_event(&mut incoming_rx, 0.5).await {
            Some((
                _conn_idx,
                WebSocketEvent::Closed {
                    reason: _reason @ ClosureReason::ClientClosedOnUnderlyingError(_),
                },
            )) => (),
            other => panic!(
                "Expected 'Closed' event with 'ClosedOnUnderlyingError' reason, found: {:?}",
                other
            ),
        }
    }

    #[wasm_bindgen_test]
    async fn test_websocket_invalid_url() {
        register_wasm_log(LogLevel::Debug);
        let conn_idx = CONN_IDX.fetch_add(1, Ordering::Relaxed);

        let _error =
            spawn_ws_transport(conn_idx, "invalid address").expect_err("!spawn_ws_transport but should be error");
        // TODO print the error when there is a way to extract the error message
        // error!("{}", error)
    }
}
