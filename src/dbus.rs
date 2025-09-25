use cosmic::iced::{self, futures::StreamExt};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

#[derive(Clone, Debug)]
pub enum Event {
    Show,
    Hide,
}

struct CosmicWorkspaces {
    event_sender: broadcast::Sender<Event>,
}

#[zbus::interface(name = "com.system76.CosmicWorkspaces")]
impl CosmicWorkspaces {
    fn show(&self) {
        let _ = self.event_sender.send(Event::Show);
    }

    fn hide(&self) {
        let _ = self.event_sender.send(Event::Hide);
    }

    #[zbus(signal)]
    async fn shown(&self, _emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn hidden(&self, _emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
}

#[derive(Clone, Debug)]
pub struct Interface {
    emitter: zbus::object_server::SignalEmitter<'static>,
    event_sender: broadcast::Sender<Event>,
}

impl Interface {
    pub async fn new(conn: zbus::Connection) -> zbus::Result<Self> {
        let event_sender = broadcast::Sender::new(8);
        conn.object_server()
            .at(
                "/com/system76/CosmicWorkspaces",
                CosmicWorkspaces {
                    event_sender: event_sender.clone(),
                },
            )
            .await?;
        Ok(Interface {
            emitter: zbus::object_server::SignalEmitter::new(
                &conn,
                "/com/system76/CosmicWorkspaces",
            )
            .unwrap(),
            event_sender,
        })
    }

    pub async fn shown(&self) -> zbus::Result<()> {
        self.emitter.shown(&self.emitter).await
    }

    pub async fn hidden(&self) -> zbus::Result<()> {
        self.emitter.hidden(&self.emitter).await
    }

    pub fn subscription(&self) -> iced::Subscription<Event> {
        iced::Subscription::run_with_id(
            "workspaces-dbus-sun",
            BroadcastStream::new(self.event_sender.subscribe()).filter_map(|x| async { x.ok() }),
        )
    }
}
